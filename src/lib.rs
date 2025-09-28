// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module is the main entry point.
//! It defines the Pluguzu struct which is instantiated by nih-plug as a standalone app or a DAW plugin.

mod config;
mod debug;
mod default_presets;
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

use arc_swap::ArcSwap;
use atomic_float::{AtomicF32, AtomicF64};
use nih_plug::prelude::*;
use nih_plug_egui::{egui, EguiState};
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize};
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::events::{Event, EventKind};
use crate::haskell::{BackgroundBuffer, HaskellState};
use crate::midi::{FutureNoteOff, ProcessAudio, ProcessMidi};

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
    /// Are we synced to an external clock?
    synced: AtomicBool,
    /// The BPM when not synced.
    bpm: AtomicF32,
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
    error: Mutex<Option<String>>,
    /// The event location for highlights.
    evlocs: ArcSwap<Vec<Event>>,
    /// The last event location position.
    evlocs_pos: AtomicUsize,
}

pub struct Pluguzu {
    /// The params saved to the DAW.
    params: Arc<PluguzuParams>,

    /// The Haskell runtime and background buffer
    haskell_state: Arc<HaskellState>,

    /// User preset
    user_presets_file: Arc<PathBuf>,

    /// Initial file when running standalone
    initial_file: Arc<Mutex<Option<String>>>,
    hosted_in_daw: Arc<AtomicBool>,

    /// The info shared to the UI.
    dsp_state: Arc<PluguzuDSP>,

    /// The pending note-offs.
    note_offs: HashMap<(u8, u8), FutureNoteOff>,

    /// Samples
    dirt: dirt::Dirt,
    samples_library: Arc<crate::samples::SampleLibrary>,

    /// The info needed to send midi events.
    midi: ProcessMidi,
    audio: ProcessAudio,
    last_timing_warning: u8,

    /// Keep track of the current playing bar so that code update are queued to the next bar.
    current_bar: Arc<AtomicI32>,

    /// The current events buffer to be sent.
    events: Option<Arc<Vec<Event>>>,
    /// The current events buffer position, e.g. the index of the last event submitted.
    events_pos: usize,
    /// The current event buffers starting bar, e.g. when it was last synchronized.
    events_start_bar: i32,

    /// The state shared between the UI and the background task executor.
    background_state: Arc<BackgroundState>,

    /// Detect the first process callback, to work around a silly bug in nih-plug.
    skip_first: bool,
}

impl Default for Pluguzu {
    fn default() -> Self {
        #[cfg(not(feature = "debug"))]
        nih_plug::log::set_max_level(nih_plug::log::LevelFilter::Info);

        let config = Config::new();
        let samples_dir = config.path("samples");
        Self {
            params: Arc::new(PluguzuParams::default()),
            haskell_state: Arc::new(HaskellState::default()),
            user_presets_file: Arc::new(config.path("presets.md")),
            initial_file: Arc::new(Mutex::new(None)),
            hosted_in_daw: Arc::new(AtomicBool::new(true)),
            events: None,

            dsp_state: Arc::new(PluguzuDSP {
                playing: AtomicBool::new(false),
                next: AtomicI32::new(0),
                remaining: AtomicUsize::new(0),
                pos: AtomicF64::new(0.),
                synced: AtomicBool::new(true),
                bpm: AtomicF32::new(120.),
            }),

            dirt: dirt::Dirt::new(8),
            samples_library: Arc::new(crate::samples::SampleLibrary::new(samples_dir)),

            note_offs: HashMap::with_capacity(255 * 16),

            midi: ProcessMidi {
                start: 0.,
                end: 0.,
                frames_per_fraction: 0.,
            },
            audio: ProcessAudio {
                cps: 0.,
                now: 0.,
                sample_rate: 44100,
            },

            current_bar: Arc::new(AtomicI32::new(0)),

            events_start_bar: -1024,
            events_pos: 0,

            background_state: Arc::new(BackgroundState {
                error: Mutex::new(None),
                evlocs: ArcSwap::from_pointee(Vec::new()),
                evlocs_pos: AtomicUsize::new(0),
            }),
            skip_first: false,
            last_timing_warning: 0,
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum ParseTask {
    InPlace,
    Next,
    NoRender,
}

pub enum PluguzuTask {
    Parse(ParseTask),
    Render(i32),
    LoadCode(PathBuf),
    SaveCode(PathBuf),
}

const BUFFER_LEN: i32 = 4;

struct TransportInfo {
    /// Whether the transport is running.
    playing: bool,
    /// The position in seconds.
    now: f64,
    /// The position in cycle fraction.
    cycle_pos: f64,
    /// The length of a cycle in seconds. This is used to know when the current buffer ends.
    cycle_length: f64,
    /// The cps, for dirt.
    cycle_per_sec: f32,
}

impl TransportInfo {
    fn new(playing: bool, now: f64, tempo: f64, sig_numerator: f64) -> TransportInfo {
        TransportInfo {
            playing,
            now,
            cycle_pos: now / 60.0 * tempo / sig_numerator,
            cycle_length: 60. / tempo * sig_numerator,
            cycle_per_sec: (tempo / 60. / sig_numerator) as f32,
        }
    }
}

// Add a tiny padding to the end, because sometime the frame don't line-up perfectly.
const BUFFER_END_PADDING: f64 = 0.0000000001;

impl Pluguzu {
    // Setup the process callback buffer.
    fn sync_events_buffer(
        &mut self,
        // The starting bar position of the buffer
        start: i32,
        buffer: &mut BackgroundBuffer,
        ctx: &impl ProcessContext<Self>,
    ) {
        // Here is the tricky part.
        // We must not de-allocate the events buffer in the audio callback.
        if let Some(stale) = buffer.stale.take() {
            // This should never happen because the background thread should always free the buffer after the sync.
            nih_plug::util::permit_alloc(move || {
                nih_warn!("Stale buffer is not free, de-allocating from audio callback...");
                std::mem::drop(stale);
            })
        }
        let buffer_start = buffer.bar_start.unwrap_or(start);
        // Store the previous events back in the buffer so that they will be freed by the background task
        buffer.stale = self.events.take();
        // Borrow the new events.
        let events = buffer.events.clone();
        self.events_pos = 0;
        self.events_start_bar = start;
        // Mark the background buffer as used.
        buffer.bar_start = None;

        // When the buffer is updated in-place, then we advance the buffer pos to skip past events
        if buffer.in_place || start > buffer_start {
            debug!("Skipping event due to forced in-place update...");
            while self.events_pos < events.len() {
                let ev = &events[self.events_pos];
                if ev.span.start >= self.midi.start {
                    break;
                }
                self.events_pos += 1;
            }
        }

        // Share the events with the UI
        if self.events.is_none() {
            // arc-swap does a small allocation the first time it is used...
            nih_plug::util::permit_alloc(|| self.background_state.evlocs.store(events.clone()));
        } else {
            self.background_state.evlocs.store(events.clone())
        }
        self.background_state.evlocs_pos.store(0, Ordering::Relaxed);

        // Keep a copy of the events for the process callback
        self.events = Some(events);

        // Request for next batch to be rendered.
        ctx.execute_background(PluguzuTask::Render(start + BUFFER_LEN))
    }

    // Make sure the events buffer is synchronized, return the end bar of the current buffer.
    fn ensure_events_buffer(&mut self, cur_bar: i32, ctx: &impl ProcessContext<Self>) -> i32 {
        let state = self.haskell_state.clone();
        let mut buffer = state.buffer.lock();
        if let Some(start) = buffer.bar_start
            && cur_bar >= start
            && cur_bar < start + BUFFER_LEN
        {
            // New events are already available.
            debug!("{}: Copying buffer data from {}", self.midi.start, start);
            self.sync_events_buffer(start, buffer.deref_mut(), ctx);
            start + BUFFER_LEN
        } else if cur_bar < self.events_start_bar || cur_bar >= self.events_start_bar + BUFFER_LEN {
            // We are out of sync, need new events
            let updated = nih_plug::util::permit_alloc(|| {
                pwarn!(
                    "{}: {cur_bar} is out of sync {}..{}, rendering the event synchronously... ",
                    self.midi.start,
                    self.events_start_bar,
                    self.events_start_bar + BUFFER_LEN
                );
                if state.rts.is_locked() {
                    // It looks like the background thread is also performing a render...
                    pwarn!("The background is locking the Haskell runtime?!");
                    false
                } else {
                    let events = state.rts.lock().render(cur_bar, cur_bar + BUFFER_LEN);
                    buffer.update(cur_bar, events, true);
                    true
                }
            });
            if updated {
                self.sync_events_buffer(cur_bar, buffer.deref_mut(), ctx);
                cur_bar + BUFFER_LEN
            } else {
                self.events_start_bar + BUFFER_LEN
            }
        } else {
            // We are in sync.
            self.events_start_bar + BUFFER_LEN
        }
    }

    fn compute_timing(&mut self, frames: usize, info: &TransportInfo) {
        // Compute current frame info
        let frames = frames as f64;
        let run_time = frames / self.dirt.sr;
        let run_fraction = run_time / info.cycle_length;
        let frames_per_fraction = frames / run_fraction;

        // Sanity checks
        if info.cycle_pos <= self.midi.start {
            // Transport jump back in time, let's reset the buffer position to replay past event.
            self.events_pos = 0;
            // Reset the highlights position too
            self.background_state.evlocs_pos.store(0, Ordering::Relaxed);
        }

        if self.last_timing_warning == 0 && info.cycle_pos > self.midi.end {
            pwarn!(
                "There is a gap with the last callback: {} > {}, gap {}",
                info.cycle_pos,
                self.midi.end,
                info.cycle_pos - self.midi.end
            );
            self.last_timing_warning = u8::MAX;
        }
        self.last_timing_warning = self.last_timing_warning.saturating_sub(1);

        // Timing debug...
        /*
        nih_plug::util::permit_alloc(|| {
            println!(
                "HERE: {} {}..{}, length {run_fraction}",
                info.sycle_pos,
                self.midi.end,
                pos + run_fraction,
            )
        });
        */
        // Uncomment the line above to observe the padding behavior.

        // Store the midi offset stuff
        self.midi.start = info.cycle_pos;
        self.midi.end = info.cycle_pos + run_fraction + BUFFER_END_PADDING;

        if frames_per_fraction != self.midi.frames_per_fraction {
            debug!("Frames per fraction changed to {}", frames_per_fraction);
            self.midi.frames_per_fraction = frames_per_fraction;
        }

        // Store timing offset for dirt
        if info.cycle_per_sec != self.audio.cps {
            debug!("CPS changed to {}", info.cycle_per_sec);
            self.audio.cps = info.cycle_per_sec;
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
        // Load file
        if context.plugin_api() == PluginApi::Standalone {
            self.hosted_in_daw.store(false, Ordering::Relaxed);
            match std::env::args().nth(1) {
                Some(fp) if !fp.starts_with("-") => {
                    *self.params.code.lock().unwrap() =
                        std::fs::read_to_string(&fp).unwrap_or_else(|_| format!("-- {fp}\n"));
                    *self.initial_file.lock().unwrap() = Some(fp);
                }
                _ => {}
            }

            self.skip_first = true;
        }

        // Parse the initial code
        context.execute(PluguzuTask::Parse(ParseTask::NoRender));

        // Setup sample-rate
        let sr = buffer_config.sample_rate as u32;
        self.dirt.set_sample_rate(buffer_config.sample_rate.into());
        self.audio.sample_rate = sr;
        self.samples_library.set_sample_rate(sr);

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

        let transport = context.transport();

        if self.skip_first {
            // Use local transport if not playing and standalone
            if !transport.playing && !self.hosted_in_daw.load(Ordering::Relaxed) {
                self.dsp_state.synced.store(false, Ordering::Relaxed);
                self.dsp_state.playing.store(true, Ordering::Relaxed);
            }

            // Sometime, in standalone mode, the very first callback happens at the wrong time.
            // Here is how it looks in debug:
            //    CPS changed to 0.5
            //    01:31:45 [WARN] pluguzu: 3885.173333333333: 3885 is out of sync -1024..-1020, rendering the event synchronously...
            //    01:31:45 [WARN] pluguzu: 722.1653333333334: 722 is out of sync 3885..3889, rendering the event synchronously...
            //    722: Rendering 3889
            //    722: Rendering 726
            //
            // For now, this is a ugly hack:)
            self.skip_first = false;
            return ProcessStatus::KeepAlive;
        }

        let synced = self.dsp_state.synced.load(Ordering::Relaxed);

        // Lookup transport and read the current bar.
        let frames_count = buffer.samples();
        let transport_info = if !synced {
            let playing = self.dsp_state.playing.load(Ordering::Relaxed);
            let tempo = self.dsp_state.bpm.load(Ordering::Relaxed) as f64;
            if playing {
                let mut info = TransportInfo::new(true, 0.0, tempo, 4.);
                // Advance the time based on the previous calculation
                info.cycle_pos = (self.midi.end - BUFFER_END_PADDING).max(0.);
                info.now = self.audio.now + frames_count as f64 / self.dirt.sr;
                info
            } else {
                self.midi.end = 0.;
                TransportInfo::new(false, 0.0, tempo, 4.)
            }
        } else {
            // Lookup external transport info.
            let now = transport.pos_seconds().expect("position");
            let tempo = transport.tempo.unwrap_or(120.);
            let sig_numerator = transport.time_sig_numerator.unwrap_or(4) as f64;
            TransportInfo::new(transport.playing, now, tempo, sig_numerator)
        };

        self.audio.now = transport_info.now;
        let pos = transport_info.cycle_pos;
        let cur_bar = pos.floor() as i32;
        self.current_bar.store(cur_bar, Ordering::Relaxed);

        // When not playing...
        if !transport_info.playing {
            // Tell the UI
            if synced {
                self.dsp_state.playing.store(false, Ordering::Relaxed);
            }

            // Reset event position.
            self.events_pos = 0;
            self.background_state.evlocs_pos.store(0, Ordering::Relaxed);

            self.send_all_note_offs(context);

            self.dirt.stop();
            return ProcessStatus::Normal;
            // return ProcessStatus::KeepAlive;
        }

        self.compute_timing(frames_count, &transport_info);

        // Send any previously registered note-off.
        self.send_note_offs(context);

        // Ensure the events buffer is ready
        let next_buffer = self.ensure_events_buffer(cur_bar, context);
        let mut events = self.events.as_ref().expect("events to exist").clone();

        // Send events.
        self.send_tidal_events(&events, context);

        // Send events from the begining of the next-buffer.
        if self.midi.end > next_buffer as f64 {
            // try to resync buffer, e.g.: when the current run overlap with the next chunk
            let state = self.haskell_state.clone();
            let mut buffer = state.buffer.lock();
            if let Some(start) = buffer.bar_start
                && next_buffer >= start
            {
                debug!(
                    "{pos:.4} process buffer overlaps with next event buffer {} > {next_buffer}",
                    self.midi.end
                );
                self.sync_events_buffer(next_buffer, buffer.deref_mut(), context);
                events = self.events.as_ref().expect("events").clone();
                self.send_tidal_events(&events, context);
            } else {
                pwarn!(
                    "{pos:.4} ERROR: the next event buffer is not ready?! {next_buffer} {:?}",
                    42
                );
            }
        }

        // Play dirt
        self.dirt
            .playback(self.audio.now, frames_count, buffer.as_slice());

        // Update status for the UI
        if synced {
            self.dsp_state
                .playing
                .store(transport_info.playing, Ordering::Relaxed);
        }
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
        let haskell_state = self.haskell_state.clone();
        let current_bar = self.current_bar.clone();
        let samples_library = self.samples_library.clone();
        let background_state = self.background_state.clone();

        let params = self.params.clone();
        let last_parse = Mutex::new(None);

        Box::new(move |task| {
            let current_bar = current_bar.load(Ordering::Relaxed);

            let load_missing_samples = |events: &[Event]| {
                let mut missing = HashSet::new();
                missing.clear();
                // Collect unknown samples
                for ev in events.iter() {
                    if let EventKind::EventSound(sev) = &ev.kind
                        && !samples_library.has_sample(sev)
                    {
                        missing.insert(sev.clone());
                    }
                }
                // Dispatch background task to load missing samples
                for sev in missing.drain() {
                    if let Err(e) = samples_library.load(&sev) {
                        nih_warn!("Couldn't load sample {}: {}", sev.name, e);
                    }
                }
            };

            let update_state = |start: i32, events: Arc<Vec<Event>>, in_place: bool| {
                {
                    let mut state = haskell_state.buffer.lock();
                    state.update(start, events.clone(), in_place);
                }
                load_missing_samples(&events)
            };

            let do_parse = |parse_task| {
                let code = code.lock().expect("lock");
                // println!("Parsing {}", code);

                // Perform code parse and maybe render
                let err = haskell_state.rts.lock().parse(&code);
                let is_err = err.is_some();
                *background_state.error.lock().expect("lock") = err;
                let render = |start, in_place| {
                    let events = haskell_state.rts.lock().render(start, start + BUFFER_LEN);

                    // Lock the process callback at the end to update the shared state with the new events.
                    debug!(
                        "{current_bar} parsed and rendered {start}..{} (in place: {in_place})",
                        start + BUFFER_LEN
                    );
                    update_state(start, events, in_place)
                };
                if !is_err {
                    match parse_task {
                        ParseTask::InPlace => render(current_bar, true),
                        ParseTask::Next => render(current_bar + 1, false),
                        ParseTask::NoRender => {}
                    }
                }
            };
            match task {
                PluguzuTask::Parse(parse_task) => {
                    let version = params.version.value();
                    let mut last_parse = last_parse.lock().unwrap();
                    if last_parse.is_none() {
                        do_parse(parse_task);
                    } else if let Some(last_parse) = *last_parse
                        && (version, parse_task) != last_parse
                    {
                        do_parse(parse_task);
                    }
                    *last_parse = Some((version, parse_task));
                }
                PluguzuTask::Render(start) => {
                    // This task happens right after the process callback got the events.
                    debug!("{current_bar}: Rendering {}", start);

                    // First acquire the Haskell runtime to render the next batch.
                    let events = haskell_state.rts.lock().render(start, start + BUFFER_LEN);

                    // Then lock the process callback to store the new events.
                    update_state(start, events, false)
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
