// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module is the main entry point.
//! It defines the Pluguzu struct which is instantiated by nih-plug as a standalone app or a DAW plugin.

mod code_editor;
mod config;
mod debug;
mod default_presets;
mod dough;
pub mod events;
pub mod haskell;
mod highlights;
mod midi;
mod presets;
mod samples;
mod soundmap;
mod syntax;
pub mod transport;
pub mod transport_debug;
mod ui;
pub(crate) use debug::{debug, info, pwarn};

use atomic_float::{AtomicF32, AtomicF64};
use nih_plug::prelude::*;
use nih_plug_egui::{EguiState, egui};
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::events::{EventKind, Events, EventsBuffer};
use crate::haskell::{BackgroundBuffer, HaskellState, UzuKind};
use crate::midi::FutureNoteOff;
use crate::transport::{CycleArc, CycleRange, CycleTransport};

/// Info from the engine for the UI status bar
struct TransportState {
    /// Is the transport playing?
    playing: AtomicBool,
    /// Count down of the remaining bar until the next events buffer start.
    remaining: AtomicUsize,
    /// The current end fraction for event highlights
    dsp_end: AtomicF64,
    dsp_start: AtomicF64,
    dsp_jumped: AtomicBool,
    /// Are we synced to an external clock?
    synced: AtomicBool,
    /// The BPM when not synced.
    bpm: AtomicF32,
}

struct PluguzuTransport {
    cycle_transport: CycleTransport,
    continuous: Option<i32>,
    range: CycleRange,
    state: Arc<TransportState>,
}

impl PluguzuTransport {
    fn update(&mut self, frames_count: i64, transport: &nih_plug::prelude::Transport) -> bool {
        let synced = self.state.synced.load(Ordering::Relaxed);

        let playing = if synced {
            if let Some(range) = self
                .cycle_transport
                .update_transport(frames_count, transport)
            {
                self.continuous = None;
                self.state
                    .playing
                    .store(transport.playing, Ordering::Relaxed);
                self.range = range;
            } else {
                pwarn!("Invalid transport?!");
            }
            transport.playing
        } else {
            let playing = self.state.playing.load(Ordering::Relaxed);
            let tempo = self.state.bpm.load(Ordering::Relaxed) as f64;
            self.range = self.cycle_transport.update_local(
                playing,
                frames_count,
                self.range.arc.end * 4.,
                tempo,
                4,
            );
            self.continuous = Some(self.range.arc.start.floor() as i32);
            if !playing {
                self.range.arc.end = 0.;
            }
            playing
        };
        self.state
            .dsp_end
            .store(self.range.arc.end, Ordering::Relaxed);
        if self.range.jumped {
            self.state
                .dsp_start
                .store(self.range.arc.start, Ordering::Relaxed);
        }
        playing
    }
}

/// The params that can be saved/loaded
#[derive(Params)]
pub struct PluguzuParams {
    /// The editor state like the window size.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    /// The current code.
    #[persist = "code"]
    code: Arc<Mutex<String>>,

    /// The uzu lang.
    #[persist = "lang"]
    lang: Arc<Mutex<UzuKind>>,

    /// The pattern length
    #[persist = "length"]
    length: Arc<AtomicU32>,

    /// The font size.
    #[persist = "font-size"]
    font_size: Arc<AtomicU8>,

    /// Code "version" number, used to tell the host when the code changed.
    #[id = "version"]
    version: IntParam,
}

impl Default for PluguzuParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(800, 600),
            code: Arc::new(Mutex::new(crate::config::WELCOME.trim_end().into())),
            lang: Arc::new(Mutex::new(UzuKind::Tidal)),
            length: Arc::new(AtomicU32::new(4)),
            font_size: Arc::new(AtomicU8::new(20)),
            version: IntParam::new(
                "version",
                0,
                IntRange::Linear {
                    min: 0,
                    max: i32::MAX,
                },
            ),
        }
    }
}

/// This structure is shared between the background executor and the UI
/// It is not accessed in the process audio callback
struct BackgroundState {
    error: Mutex<Option<String>>,
    error_row: AtomicUsize,
    error_col: AtomicUsize,
    events: Mutex<Arc<Events>>,
    events_ready: AtomicBool,
}

pub struct Pluguzu {
    /// The params saved to the DAW.
    params: Arc<PluguzuParams>,

    /// The internal transport
    transport: PluguzuTransport,

    /// The Haskell runtime and background buffer
    haskell_state: Arc<HaskellState>,

    /// User preset
    user_presets_file: Arc<PathBuf>,

    /// Initial file when running standalone
    initial_file: Arc<Mutex<Option<String>>>,
    hosted_in_daw: Arc<AtomicBool>,

    /// The pending note-offs.
    note_offs: HashMap<(u8, u8), FutureNoteOff>,

    /// Samples
    dough: crate::dough::Dough,
    samples_library: Arc<crate::samples::SampleLibrary>,

    events: EventsBuffer,

    /// The state shared between the UI and the background task executor.
    background_state: Arc<BackgroundState>,
}

impl Default for Pluguzu {
    fn default() -> Self {
        #[cfg(not(feature = "debug"))]
        nih_plug::log::set_max_level(nih_plug::log::LevelFilter::Info);

        let config = Config::new();
        let samples_dir = config.path("samples");
        let sm = soundmap::load_soundmaps(samples_dir).unwrap_or_else(|e| {
            pwarn!("Couldn't load sound map: {:?}", e);
            soundmap::SoundMap(HashMap::new())
        });
        Self {
            params: Arc::new(PluguzuParams::default()),
            transport: PluguzuTransport {
                cycle_transport: CycleTransport::default(),
                range: CycleRange {
                    jumped: true,
                    arc: CycleArc { start: 0., end: 0. },
                },
                state: Arc::new(TransportState {
                    playing: AtomicBool::new(false),
                    remaining: AtomicUsize::new(0),
                    dsp_end: AtomicF64::new(0.),
                    dsp_start: AtomicF64::new(0.),
                    dsp_jumped: AtomicBool::new(true),
                    synced: AtomicBool::new(true),
                    bpm: AtomicF32::new(120.),
                }),
                continuous: None,
            },
            haskell_state: Arc::new(HaskellState::default()),
            user_presets_file: Arc::new(config.path("presets.md")),
            initial_file: Arc::new(Mutex::new(None)),
            hosted_in_daw: Arc::new(AtomicBool::new(true)),
            events: EventsBuffer::default(),

            dough: dough::Dough::new(),
            samples_library: Arc::new(crate::samples::SampleLibrary::new(sm)),

            note_offs: HashMap::with_capacity(255 * 16),

            background_state: Arc::new(BackgroundState {
                error: Mutex::new(None),
                error_row: AtomicUsize::new(0),
                error_col: AtomicUsize::new(0),
                events: Mutex::new(Arc::new(Events::default())),
                events_ready: AtomicBool::new(false),
            }),
        }
    }
}

pub enum PluguzuTask {
    Update,
    RenderNext(i32),
    LoadCode(PathBuf),
    SaveCode(PathBuf),
}

impl Pluguzu {
    // Setup the process callback buffer.
    fn sync_events_buffer(&mut self, buffer: &mut BackgroundBuffer) {
        // Here is the tricky part.
        // We must not de-allocate the events buffer in the audio callback.
        if let Some(stale) = buffer.stale.take() {
            // This should never happen because the background thread should always free the buffer after the sync.
            nih_plug::util::permit_alloc(move || {
                nih_warn!("Stale buffer is not free, de-allocating from audio callback...");
                std::mem::drop(stale);
            })
        }
        // Store the previous events back in the buffer so that they will be freed by the background task
        buffer.stale = Some(self.events.take());

        let cycle_length = if self.transport.continuous.is_some() {
            None
        } else {
            Some(self.params.length.load(Ordering::Relaxed) as f64)
        };
        self.events.reset(buffer.events.clone(), cycle_length);
    }

    fn ensure_events_buffer(&mut self) -> bool {
        // Copy the buffer from the haskell thread if needed
        if self.haskell_state.updated.load(Ordering::Relaxed) {
            if let Some(current_bar) = self.transport.continuous
                && current_bar < self.haskell_state.buffer_start.load(Ordering::Relaxed)
            {
                // continuous transport waiting for the current bar to match next buffer
                return false;
            }
            let state = self.haskell_state.clone();
            let mut buffer = nih_plug::util::permit_alloc(|| state.buffer.lock());
            self.sync_events_buffer(buffer.deref_mut());
            self.haskell_state.updated.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
    pub fn send_tidal_events(
        &mut self,
        buffer: &mut [&mut [f32]],
        context: &mut impl ProcessContext<Self>,
    ) {
        // #[rustfmt::skip]
        // debug!("{start:.7}..{end:.7} Sending events ({}/{}) (for {})", self.events_pos, events.len(), self.transport.range.arc);
        let mut events = self.events.iter(self.transport.range);
        while let Some((ev, span_start)) = events.next_ev() {
            match &ev.kind {
                EventKind::EventSound(sev) => self.dough.play_sound_event(
                    &self.transport,
                    &self.samples_library,
                    ev,
                    span_start,
                    sev,
                    buffer,
                ),
                EventKind::EventMidi(mev) => self.transport.send_midi_event(
                    &ev.span,
                    span_start,
                    mev,
                    &mut self.note_offs,
                    context,
                ),
            }
        }
    }
}

fn path_to_lang(path: &str) -> UzuKind {
    if path.ends_with(".mondo") {
        UzuKind::Mondo
    } else {
        UzuKind::Tidal
    }
}

impl Plugin for Pluguzu {
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        // Load file on standalone start
        if context.plugin_api() == PluginApi::Standalone {
            self.transport.state.synced.store(false, Ordering::Relaxed);
            self.hosted_in_daw.store(false, Ordering::Relaxed);
            match std::env::args().nth(1) {
                Some(fp) if !fp.starts_with("-") => {
                    let lang = path_to_lang(&fp);
                    *self.params.code.lock().unwrap() = std::fs::read_to_string(&fp)
                        .unwrap_or_else(|_| match lang {
                            UzuKind::Tidal => format!("-- {fp}\n"),
                            UzuKind::Mondo => format!("// {fp}\n"),
                        });
                    *self.params.lang.lock().unwrap() = lang;
                    *self.initial_file.lock().unwrap() = Some(fp);
                }
                _ => {}
            }
        }

        // Parse the initial code
        context.execute(PluguzuTask::Update);

        // Setup sample-rate
        let sr = buffer_config.sample_rate as u32;
        self.dough.initialize(sr.into());
        self.samples_library.set_sample_rate(sr);
        self.transport.cycle_transport.sample_rate = sr as f64;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Update transport timings
        let nih_transport = context.transport();
        let frames_count = buffer.samples();
        let running = self.transport.update(frames_count as i64, nih_transport);

        // Pass through midi events:
        while let Some(event) = context.next_event() {
            self.events
                .midi_trigger
                .update_from_event(&self.transport, &event);
            context.send_event(event)
        }

        // When not playing...
        if !running {
            self.transport
                .send_all_note_offs(context, &mut self.note_offs);
            self.dough.stop();
            if self.transport.continuous.is_some()
                && self.haskell_state.buffer_start.load(Ordering::Relaxed) > 0
            {
                // Re-render for first cycle.
                context.execute_background(PluguzuTask::RenderNext(0))
            }
            return ProcessStatus::Normal;
            // return ProcessStatus::KeepAlive;
        }

        // Send any previously registered note-off.
        self.transport.send_note_offs(context, &mut self.note_offs);

        // Ensure the events buffer is ready
        let new_events = self.ensure_events_buffer();
        if self.transport.range.jumped {
            self.dough.stop();
            self.transport
                .send_all_note_offs(context, &mut self.note_offs);
        } else if new_events {
            // Pretent transport jumped to reset buffer position...
            self.transport.range.jumped = true
        }

        if self.transport.range.jumped {
            // Tell the UI to reset its buffer
            self.transport
                .state
                .dsp_jumped
                .store(true, Ordering::Relaxed)
        }
        if new_events {
            // Tell the UI the buffer is ready
            self.background_state
                .events_ready
                .store(true, Ordering::Relaxed);
        }

        if let Some(current_bar) = self.transport.continuous
            && current_bar - self.haskell_state.buffer_start.load(Ordering::Relaxed) > 1
        {
            // Prepare the next buffer...
            context.execute_background(PluguzuTask::RenderNext(current_bar + 1))
        }

        self.dough.init_buffer(frames_count);
        let buffer_slice = buffer.as_slice();

        // Send events.
        self.send_tidal_events(buffer_slice, context);

        // Update the remaining value for the UI.
        self.transport
            .state
            .remaining
            .store(self.events.remaining(), Ordering::Relaxed);

        // Play dough
        self.dough.advance(frames_count, buffer_slice);

        ProcessStatus::KeepAlive
    }

    type BackgroundTask = PluguzuTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
        let lang = self.params.lang.clone();
        let cycle_count = self.params.length.clone();
        let haskell_state = self.haskell_state.clone();
        let samples_library = self.samples_library.clone();
        let background_state = self.background_state.clone();
        let transport_state = self.transport.state.clone();
        let first_updates = AtomicU8::new(0);

        let (tx, rx) = std::sync::mpsc::channel();
        let sl = self.samples_library.clone();
        std::thread::Builder::new()
            .name("sample-loader".to_string())
            .spawn(move || {
                loop {
                    let (bank, sev): (smol_str::SmolStr, events::SoundEvent) = rx.recv().unwrap();
                    if !sl.has_sample(bank.clone(), &sev)
                        && let Err(e) = sl.load_sample(bank, &sev)
                    {
                        nih_warn!("Couldn't load sample {}: {}", sev.name, e);
                    }
                }
            })
            .expect("Sample loader thread");

        Box::new(move |task| {
            let load_missing_samples = |events: &Events| {
                let mut missing = HashSet::new();
                missing.clear();
                for evs in &events.per_trigger {
                    // Collect unknown samples
                    for ev in evs.iter() {
                        if let EventKind::EventSound(sev) = &ev.kind
                            && !dough::is_synth_sound(sev.name.as_str())
                            && !samples_library.has_sample(ev.bank(), sev)
                        {
                            missing.insert((ev.bank(), sev.clone()));
                        }
                    }
                }
                // Dispatch task to load missing samples
                for (bank, sev) in missing.drain() {
                    if let Err(e) = tx.send((bank, sev)) {
                        nih_warn!("Couldn't load sample: {}", e);
                    }
                }
            };

            let cycle_count = cycle_count.load(Ordering::Relaxed) as i32;
            let do_render = |start| {
                let end = start + cycle_count;
                let events = haskell_state.rts.lock().render(start, end);
                pwarn!("Rendered {start}..{end} into {} events", events.count);

                // Share the events with the UI.
                {
                    background_state
                        .events_ready
                        .store(false, Ordering::Relaxed);
                    let mut ui_buf = background_state.events.lock().expect("lock");
                    *ui_buf = events.clone();
                }

                haskell_state.buffer_start.store(start, Ordering::Relaxed);

                // Lock the process callback at the end to update the shared state with the new events.
                {
                    let mut bg_buf = haskell_state.buffer.lock();
                    bg_buf.update(events.clone());
                    haskell_state.updated.store(true, Ordering::Relaxed);
                }

                // Auto play on first update (skipping the initial one on load)
                let updates = first_updates.load(Ordering::Relaxed);
                if updates == 1 && !transport_state.synced.load(Ordering::Relaxed) {
                    transport_state.playing.store(true, Ordering::Relaxed);
                }
                if updates < 2 {
                    first_updates.store(updates + 1, Ordering::Relaxed);
                }
                load_missing_samples(&events)
            };
            let do_update = || {
                let code = code.lock().expect("lock");
                let kind = *lang.lock().expect("lock");
                // println!("Parsing {}", code);

                // Perform code parse and maybe render
                let err = haskell_state.rts.lock().parse(kind, &code);
                let is_err = match err.as_ref() {
                    Some((err, row, col)) => {
                        pwarn!("Parsing failed: {row}:{col} - {err}");
                        background_state.error_row.store(*row, Ordering::SeqCst);
                        background_state.error_col.store(*col, Ordering::SeqCst);
                        true
                    }
                    None => false,
                };
                *background_state.error.lock().expect("lock") = err.map(|(err, _, _)| err);
                if !is_err {
                    let start = if transport_state.synced.load(Ordering::Relaxed) {
                        0
                    } else {
                        transport_state.dsp_end.load(Ordering::Relaxed).floor() as i32
                    };
                    do_render(start)
                }
            };

            match task {
                PluguzuTask::Update => do_update(),
                PluguzuTask::RenderNext(start) => {
                    do_render(start);
                }
                PluguzuTask::SaveCode(path) => {
                    let code = code.lock().expect("lock");
                    if let Err(e) = std::fs::write(&path, code.as_bytes()) {
                        nih_warn!("{}: couldn't save {e}", path.display())
                    } else {
                        info!("{}: saved!", path.display())
                    }
                }
                PluguzuTask::LoadCode(path) => {
                    match std::fs::read_to_string(&path) {
                        Ok(new_code) => {
                            if !new_code.is_empty() {
                                *code.lock().expect("lock") = new_code;
                                *lang.lock().expect("lock") =
                                    path_to_lang(&path.display().to_string());
                                do_update()
                            }
                        }
                        Err(e) => nih_warn!("{}: couldn't load: {e}", path.display()),
                    };
                }
            }
        })
    }

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        self.do_editor(async_executor)
    }

    type SysExMessage = ();

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            // This is also the default and can be omitted here
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    const NAME: &'static str = "Pluguzu";
    const VENDOR: &'static str = "Midirus";
    const URL: &'static str = "https://codeberg.org/TristanCacqueray/pluguzu";
    const EMAIL: &'static str = "tristan@midirus.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
}

impl ClapPlugin for Pluguzu {
    const CLAP_ID: &'static str = "com.midirus.pluguzu";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("uzu-lang live editor");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for Pluguzu {
    const VST3_CLASS_ID: [u8; 16] = *b"PluguzuYeahBoyyy";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Generator];
}

#[cfg(not(feature = "debug-plugin"))]
nih_export_clap!(Pluguzu);
#[cfg(not(feature = "debug-plugin"))]
nih_export_vst3!(Pluguzu);
