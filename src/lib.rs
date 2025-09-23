// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module is the main entry point.
//! It defines the Pluguzu struct which is instantiated by nih-plug as a standalone app or a DAW plugin.

mod config;
mod debug;
mod dirt;
mod events;
pub mod haskell;
mod highlights;
mod midi;
mod presets;
mod samples;
mod syntax;
mod ui;
pub(crate) use debug::{debug, info, pwarn};

use atomic_float::AtomicF64;
use nih_plug::prelude::*;
use nih_plug_egui::{egui, EguiState};
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize};
use std::sync::{Arc, Mutex, RwLock};

use crate::config::Config;
use crate::events::{Event, EventKind};
use crate::haskell::{HaskellRuntime, HaskellState};
use crate::midi::{FutureNoteOff, ProcessMidi};

/// Info from the engine for the UI status bar
struct PluguzuDSP {
    /// Is the transport playing?
    playing: AtomicBool,
    /// When is the next events buffer starting.
    next: AtomicI32,
    /// Count down of the remaining bar until the next events buffer start.
    remaining: AtomicUsize,
    /// The current start fraction for event highlights
    pos: AtomicF64,
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
/// It is not accessed nor lock in the process audio callback
struct BackgroundState {
    /// The current code parsing error.
    error: Option<String>,
    /// The event location for highlights.
    evlocs: Arc<Vec<Event>>,
    /// The last event location position.
    evlocs_pos: usize,
}

pub struct Pluguzu {
    /// The params saved to the DAW.
    params: Arc<PluguzuParams>,

    /// The Haskell runtime, used to parse and render events. This is not used by the process callback.
    /// Once the events are generated, they are written to the haskell_state.
    haskell_runtime: Arc<Mutex<HaskellRuntime>>,

    /// The Haskell binding state. This is locked by the process callback to check for new events.
    /// This must not be held longer than necessary by the background task.
    /// Maybe a Mutex is not necessary, something like a crossbeam channels or a UnsafeRefCell could be used instead?
    haskell_state: Arc<Mutex<HaskellState>>,

    /// User config
    config: Arc<Config>,

    /// Initial file when running standalone
    initial_file: Arc<Mutex<Option<String>>>,
    hosted_in_daw: Arc<AtomicBool>,

    /// The info shared to the UI.
    dsp_state: Arc<PluguzuDSP>,

    /// The pending note-offs.
    note_offs: HashMap<u8, FutureNoteOff>,

    /// Samples
    dirt: dirt::Dirt,
    samples_library: Arc<RwLock<crate::samples::SampleLibrary>>,
    samples_missing: HashSet<crate::events::SoundEvent>,

    /// The info needed to send midi events.
    midi: ProcessMidi,
    /// The cycle per second, for dirt.
    cps: f32,
    now: f64,

    /// Keep track of the current playing bar so that code update are queued to the next bar.
    current_bar: Arc<AtomicI32>,

    /// The current event buffers to be sent.
    events: Option<Arc<Vec<Event>>>,
    /// The last event buffer position.
    events_pos: usize,
    /// When the event buffers was last synchronized.
    events_start_bar: i32,

    background_state: Arc<Mutex<BackgroundState>>,
}

impl Default for Pluguzu {
    fn default() -> Self {
        let config = Config::new();
        let samples_dir = config.path("samples");
        Self {
            params: Arc::new(PluguzuParams::default()),
            haskell_state: Arc::new(Mutex::new(HaskellState::default())),
            haskell_runtime: Arc::new(Mutex::new(HaskellRuntime::default())),
            config: Arc::new(config),
            initial_file: Arc::new(Mutex::new(None)),
            hosted_in_daw: Arc::new(AtomicBool::new(true)),
            events: None,

            dsp_state: Arc::new(PluguzuDSP {
                playing: AtomicBool::new(false),
                next: AtomicI32::new(0),
                remaining: AtomicUsize::new(0),
                pos: AtomicF64::new(0.),
            }),

            dirt: dirt::Dirt::new(8),
            samples_missing: HashSet::with_capacity(42),
            samples_library: Arc::new(RwLock::new(crate::samples::SampleLibrary::new(samples_dir))),

            note_offs: HashMap::with_capacity(255),

            midi: ProcessMidi {
                start: 0.,
                end: 0.,
                frames_per_fraction: 0.,
            },
            cps: 0.,
            now: 0.,

            current_bar: Arc::new(AtomicI32::new(0)),

            events_start_bar: -1024,
            events_pos: 0,

            background_state: Arc::new(Mutex::new(BackgroundState {
                error: None,
                evlocs: Arc::new(Vec::new()),
                evlocs_pos: 0,
            })),
        }
    }
}

#[derive(Copy, Clone)]
pub enum ParseTask {
    InPlace,
    Next,
    NoRender,
}

pub enum PluguzuTask {
    Parse(ParseTask),
    Render(i32),
    LoadSample(crate::events::SoundEvent),
    LoadCode(PathBuf),
    SaveCode(PathBuf),
}

const BUFFER_LEN: i32 = 4;

impl Pluguzu {
    fn sync_events_buffer(
        &mut self,
        state: &mut HaskellState,
        start: i32,
        ctx: &impl ProcessContext<Self>,
    ) {
        // Here is the tricky part.
        // We must not de-allocate the events buffer in the audio callback.
        if let Some(stale) = state.stale.take() {
            // This should never happen because the background thread should always free the buffer after the sync.
            nih_plug::util::permit_alloc(move || {
                nih_warn!("Stale buffer is not free, de-allocating from audio callback...");
                std::mem::drop(stale);
            })
        }
        // So we lend back the forground buffer to avoid dropping it ourself.
        state.stale = self.events.take();
        // And we borrow the new ones.
        let events = state.events.clone();
        self.events_pos = 0;
        self.events_start_bar = start;
        state.bar_start = None;

        if state.in_place {
            debug!("Skipping event due to forced in-place update...");
            while self.events_pos < events.len() {
                let ev = &events[self.events_pos];
                if ev.span.start >= self.midi.end {
                    break;
                }
                self.events_pos += 1;
            }
        }

        // Pre Load samples
        // TODO: this could be done outside of the state lock...
        {
            self.samples_missing.clear();
            let lib = self.samples_library.read().expect("lock");
            // Collect unknown samples
            for ev in events.iter() {
                if let EventKind::EventSound(sev) = &ev.kind
                    && !lib.has_sample(sev)
                {
                    nih_plug::util::permit_alloc(|| self.samples_missing.insert(sev.clone()));
                }
            }
            // Dispatch background task to load missing samples
            for missing in self.samples_missing.drain() {
                ctx.execute_background(PluguzuTask::LoadSample(missing));
            }
        }
        self.events = Some(events);
        ctx.execute_background(PluguzuTask::Render(start + BUFFER_LEN))
    }

    fn ensure_events_buffer(&mut self, cur_bar: i32, ctx: &impl ProcessContext<Self>) -> i32 {
        let state = self.haskell_state.clone();
        let mut state = state.lock().expect("lock");
        if let Some(start) = state.bar_start
            && cur_bar >= start
            && cur_bar < start + BUFFER_LEN
        {
            // new events are already avail
            debug!("{}: Copying buffer data from {}", self.midi.start, start);
            self.sync_events_buffer(state.deref_mut(), start, ctx);
            start + BUFFER_LEN
        } else if cur_bar < self.events_start_bar || cur_bar >= self.events_start_bar + BUFFER_LEN {
            // We are out of sync, need new events
            nih_plug::util::permit_alloc(|| {
                pwarn!(
                    "{}: {cur_bar} is out of sync {}..{}, rendering the event synchronously... ",
                    self.midi.start,
                    self.events_start_bar,
                    self.events_start_bar + BUFFER_LEN
                );
                // TODO: it might be possible that the events are being generated right now in the background.
                // so if the runtime is already locked, then maybe we could skip the render now?
                let mut runtime = self.haskell_runtime.lock().expect("lock");
                let events = runtime.render(cur_bar, cur_bar + BUFFER_LEN);
                state.update(cur_bar, events, &mut runtime, false);
            });
            self.sync_events_buffer(state.deref_mut(), cur_bar, ctx);
            cur_bar + BUFFER_LEN
        } else {
            state
                .bar_start
                .unwrap_or(self.events_start_bar + BUFFER_LEN)
        }
    }
}

impl Plugin for Pluguzu {
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        #[cfg(not(feature = "debug"))]
        nih_plug::log::set_max_level(nih_plug::log::LevelFilter::Info);
        // Load file
        if context.plugin_api() == PluginApi::Standalone {
            self.hosted_in_daw.store(false, Ordering::Relaxed);
            let default_file = std::env::args()
                .nth(1)
                .map(|s| s.into())
                .unwrap_or_else(|| self.config.path("default.txt"));
            if std::fs::exists(&default_file).unwrap_or(false) {
                *self.params.code.lock().unwrap() = std::fs::read_to_string(&default_file).unwrap();
            }
        }

        context.execute(PluguzuTask::Parse(ParseTask::NoRender));
        let sr = buffer_config.sample_rate as u32;
        self.dirt.set_sample_rate(sr);
        self.samples_library.write().expect("lock").sample_rate = sr;
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Pass through midi events:
        while let Some(event) = context.next_event() {
            context.send_event(event)
        }

        // Lookup transport and read the current bar.
        let frames_usize = buffer.samples();
        let transport = context.transport();
        let playing = transport.playing;
        self.now = transport.pos_beats().expect("beat");
        let pos = self.now / 4.0;
        let cur_bar = pos.floor() as i32;
        self.current_bar.store(cur_bar, Ordering::Relaxed);

        // When not playing...
        if !playing || transport.preroll_active.unwrap_or(false) {
            {
                self.dsp_state.playing.store(false, Ordering::Relaxed);
                // Advance the time for the highlights
                self.dsp_state.pos.fetch_add(0.2, Ordering::Relaxed);
            }

            // Reset event position.
            self.events_pos = 0;

            self.send_all_note_offs(context);
            return ProcessStatus::Normal;
            // return ProcessStatus::KeepAlive;
        }

        // Compute current frame info
        let pos = transport.pos_beats().expect("pos beats") / 4.0;
        let frames = frames_usize as f64;
        let beat_per_min = transport.tempo.unwrap_or(120.);
        let beat_per_sec = 60. / beat_per_min;
        let sig_numerator = transport.time_sig_numerator.unwrap_or(4) as f64;
        let bar_length = beat_per_sec * sig_numerator;
        let run_time = frames / (transport.sample_rate as f64);
        let run_fraction = run_time / bar_length;
        let frames_per_fraction = frames / run_fraction;

        // Store the midi offset stuff
        self.midi.start = pos;
        self.midi.end = pos + run_fraction;
        if frames_per_fraction != self.midi.frames_per_fraction {
            debug!("Frames per fraction changed to {}", frames_per_fraction);
            self.midi.frames_per_fraction = frames_per_fraction;
        }

        // Store timing offset for dirt
        let cps = (beat_per_min / 60. / sig_numerator) as f32;
        if cps != self.cps {
            debug!("CPS changed to {}", cps);
            self.cps = cps;
        }

        // Send any previously registered note-off.
        self.send_note_offs(context);

        // Ensure the events buffer is ready
        let next_buffer = self.ensure_events_buffer(cur_bar, context);
        let mut events = self.events.as_ref().expect("events to exist").clone();

        // Send events.
        self.send_tidal_events(self.midi.start, self.midi.end, &events, context);

        // Send events from the begining of the next-buffer.
        if self.midi.end > next_buffer as f64 {
            // try to resync buffer, e.g.: when the current run overlap with the next chunk
            let haskell_state = self.haskell_state.clone();
            let mut haskell_state = haskell_state.lock().expect("lock");
            if let Some(start) = haskell_state.bar_start
                && next_buffer >= start
            {
                debug!(
                    "{pos:.4} process buffer overlaps with next event buffer {} > {next_buffer}",
                    self.midi.end
                );
                self.sync_events_buffer(haskell_state.deref_mut(), next_buffer, context);
                events = self.events.as_ref().expect("events").clone();
                self.send_tidal_events(self.midi.start, self.midi.end, &events, context);
            } else {
                pwarn!(
                    "{pos:.4} ERROR: the next event buffer is not ready?! {next_buffer} {:?}",
                    haskell_state.bar_start
                );
            }
        }

        // Play dirt
        self.dirt
            .playback(self.now, frames_usize, buffer.as_slice());

        // Update status for the UI
        self.dsp_state.playing.store(playing, Ordering::Relaxed);
        let next = next_buffer - cur_bar;
        self.dsp_state.next.store(next, Ordering::Relaxed);
        let remaining = events.len() - self.events_pos;
        self.dsp_state.remaining.store(remaining, Ordering::Relaxed);
        self.dsp_state.pos.store(self.midi.start, Ordering::Relaxed);
        ProcessStatus::KeepAlive
    }

    type BackgroundTask = PluguzuTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
        let haskell_runtime = self.haskell_runtime.clone();
        let haskell_state = self.haskell_state.clone();
        let current_bar = self.current_bar.clone();
        let samples_library = self.samples_library.clone();
        let background_state = self.background_state.clone();

        Box::new(move |task| {
            let current_bar = current_bar.load(Ordering::Relaxed);
            let do_parse = |parse_task| {
                // Acquire the Haskell runtime.
                let mut runtime = haskell_runtime.lock().expect("lock");

                let code = code.lock().expect("lock");
                // println!("Parsing {}", code);

                // Perform code parse and maybe render
                let err = runtime.parse(&code);
                let is_err = err.is_some();
                background_state.lock().expect("lock").error = err;
                let render = |runtime: &mut HaskellRuntime, start, in_place| {
                    let events = runtime.render(start, start + BUFFER_LEN);
                    // Lock the process callback at the end to update the shared state with the new events.
                    debug!(
                        "{current_bar} parsed and rendered {start}..{} (in place: {in_place})",
                        start + BUFFER_LEN
                    );
                    haskell_state
                        .lock()
                        .expect("lock")
                        .update(start, events, runtime, in_place);
                };
                if !is_err {
                    match parse_task {
                        ParseTask::InPlace => render(&mut runtime, current_bar, true),
                        ParseTask::Next => render(&mut runtime, current_bar + 1, false),
                        ParseTask::NoRender => {}
                    }
                }
            };
            match task {
                PluguzuTask::Parse(parse_task) => do_parse(parse_task),
                PluguzuTask::Render(start) => {
                    // This task happens right after the process callback got the events.
                    debug!("{current_bar}: Rendering {}", start);

                    // First acquire the Haskell runtime to render the next batch.
                    let mut haskell_runtime = haskell_runtime.lock().expect("lock");
                    let next_events = haskell_runtime.render(start, start + BUFFER_LEN);

                    // Also acquire the background state tot update the evlocs for highlights...
                    let mut bg_state = background_state.lock().expect("lock");
                    bg_state.evlocs_pos = 0;

                    // Finally lock the process callback to store the new events.
                    let mut haskell_state = haskell_state.lock().expect("lock");

                    // Copy the events that are used in the process callback for the evlocs.
                    // note: this sometime doesn't work on the first cycle when the transport has jitter, see:
                    //  CPS changed to 0.5
                    //  01:31:45 [WARN] pluguzu: 3885.173333333333: 3885 is out of sync -1024..-1020, rendering the event synchronously...
                    //  01:31:45 [WARN] pluguzu: 722.1653333333334: 722 is out of sync 3885..3889, rendering the event synchronously...
                    //  722: Rendering 3889
                    //  722: Rendering 726
                    // ^ it looks like the render are queued and this causes a desync between the locs and the events?
                    bg_state.evlocs = haskell_state.events.clone();
                    // And update the shared state.
                    haskell_state.update(start, next_events, &mut haskell_runtime, false)
                }
                PluguzuTask::LoadSample(sev) => {
                    let mut lib = samples_library.write().expect("lock");
                    if let Err(e) = lib.load(&sev) {
                        nih_warn!("Couldn't load sample {}: {}", sev.name, e);
                    }
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
                                do_parse(ParseTask::InPlace)
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

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

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

nih_export_clap!(Pluguzu);
nih_export_vst3!(Pluguzu);
