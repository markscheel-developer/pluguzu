// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

pub mod binding;
mod highlights;
mod midi;
mod presets;
mod syntax;
mod ui;

use nih_plug::prelude::*;
use nih_plug_egui::{egui, EguiState};
use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU8};
use std::sync::{Arc, Mutex, RwLock};

use crate::binding::PluguzuState;
use crate::midi::{FutureNoteOff, ProcessMidi};

/// Info from the engine for the UI status bar
struct PluguzuDSP {
    /// The current code parsing error.
    error: Option<String>,
    /// Is the transport playing?
    playing: bool,
    /// When is the next events buffer starting.
    next: i32,
    /// Count down of the remaining bar until the next events buffer start.
    remaining: usize,
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
}

impl Default for PluguzuParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(800, 600),
            code: Arc::new(Mutex::new(
                // "n \"c2*4\""
                "fast \"<1 2>\" $ n \"[c2 c3]\""
                    // "0123456789abcdefghji\nabcdefghji0123456789"
                    .into(),
            )),
            font_size: Arc::new(AtomicU8::new(25)),
        }
    }
}

pub struct Pluguzu {
    /// The params saved to the DAW.
    params: Arc<PluguzuParams>,

    /// The Haskell binding state.
    state: Arc<Mutex<PluguzuState>>,

    /// The info shared to the UI.
    dsp_state: Arc<RwLock<PluguzuDSP>>,

    /// The pending note-offs.
    note_offs: HashMap<u8, FutureNoteOff>,

    /// The info needed to send midi events.
    midi: ProcessMidi,

    /// Keep track of the current playing bar so that code update are queued to the next bar.
    current_bar: Arc<AtomicI32>,

    buffer: Vec<binding::MidiEvent>,
    buffer_pos: usize,
    buffer_start_bar: i32,

    evlocs: Arc<RwLock<Vec<highlights::EventLoc>>>,
    evlocs_pos: Arc<AtomicU32>,
    current_pos: Arc<RwLock<u64>>,
}

impl Default for Pluguzu {
    fn default() -> Self {
        Self {
            params: Arc::new(PluguzuParams::default()),
            state: Arc::new(Mutex::new(PluguzuState::new(512))),

            dsp_state: Arc::new(RwLock::new(PluguzuDSP {
                playing: false,
                error: None,
                next: 0,
                remaining: 0,
            })),

            note_offs: HashMap::with_capacity(255),

            midi: ProcessMidi {
                start: 0.,
                end: 0.,
                frames_per_fraction: 0.,
            },

            current_bar: Arc::new(AtomicI32::new(0)),

            buffer_start_bar: -1024,
            buffer_pos: 0,
            buffer: Vec::with_capacity(512),

            evlocs: Arc::new(RwLock::new(Vec::with_capacity(1024))),
            evlocs_pos: Arc::new(AtomicU32::new(0)),
            current_pos: Arc::new(RwLock::new(0)),
        }
    }
}

pub enum PluguzuTask {
    Parse(bool),
    Render(i32),
    UpdateHighlights,
}

const BUFFER_LEN: i32 = 4;

impl Pluguzu {
    fn sync_state_buffer(
        &mut self,
        state: &mut PluguzuState,
        start: i32,
        ctx: &impl ProcessContext<Self>,
    ) {
        self.buffer.clear();
        self.buffer.extend_from_slice(&state.buffer);
        self.buffer_pos = 0;
        self.buffer_start_bar = start;
        state.bar_start = None;

        // println!("DEBUG EVENTS");
        // for ev in &self.buffer {
        //     println!("-> {ev:?} {:?}", NoteEvent::<()>::from_midi(0, &ev.data));
        // }

        ctx.execute_background(PluguzuTask::UpdateHighlights);
        ctx.execute_background(PluguzuTask::Render(start + BUFFER_LEN))
    }
}

impl Plugin for Pluguzu {
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        context.execute(PluguzuTask::Parse(false));
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
        let transport = context.transport();
        let playing = transport.playing;
        let cur_bar = transport.bar_number().unwrap_or(0);
        self.current_bar.store(cur_bar, Ordering::Relaxed);

        // When not playing...
        if !playing || transport.preroll_active.unwrap_or(false) {
            self.dsp_state.write().unwrap().playing = false;

            // Reset event position.
            self.buffer_pos = 0;

            // Advance the time for the highlights
            *self.current_pos.write().unwrap() += 200;

            self.send_all_note_offs(context);
            return ProcessStatus::Normal;
            // return ProcessStatus::KeepAlive;
        }

        // Ensure the events buffer is ready
        {
            let state = self.state.clone();
            let mut state = state.lock().unwrap();
            if cur_bar < self.buffer_start_bar || cur_bar >= self.buffer_start_bar + BUFFER_LEN {
                // Compute current frame info
                let pos = transport.pos_beats().unwrap() / 4.0;
                println!(
                    "{pos:.4}: We are out of sync {}..{}, rendering the event synchronously... ",
                    self.buffer_start_bar,
                    self.buffer_start_bar + BUFFER_LEN
                );
                state.render(cur_bar, cur_bar + BUFFER_LEN);
                self.sync_state_buffer(state.deref_mut(), cur_bar, context);
            } else if let Some(start) = state.bar_start
                && cur_bar >= start
            {
                println!("{cur_bar}: Copying buffer data from {}", start);
                self.sync_state_buffer(state.deref_mut(), start, context);
            }
        }

        // Compute current frame info
        let pos = transport.pos_beats().unwrap() / 4.0;

        // Write the current position for events highlight
        *self.current_pos.write().unwrap() = (pos * 1000.) as u64;

        let frames = buffer.samples() as f64;
        let beat_per_sec = 60. / transport.tempo.unwrap_or(120.);
        let bar_length = beat_per_sec * (transport.time_sig_numerator.unwrap_or(4) as f64);
        let run_time = frames / (transport.sample_rate as f64);
        let run_fraction = run_time / bar_length;
        let frames_per_fraction = frames / run_fraction;
        let start_fraction = pos;
        let stop_fraction = pos + run_fraction;

        // Store the midi offset stuff
        self.midi.start = start_fraction;
        self.midi.end = stop_fraction;
        if frames_per_fraction != self.midi.frames_per_fraction {
            println!("Frames per fraction changed to {}", frames_per_fraction);
            self.midi.frames_per_fraction = frames_per_fraction;
        }

        // Send any previously registered note-off.
        self.send_note_offs(context);

        // Send events.
        self.send_events(start_fraction, stop_fraction, context);

        // Send events from the begining of the next-buffer.
        let next_buffer = self.buffer_start_bar + BUFFER_LEN;
        if self.buffer_pos == self.buffer.len() && stop_fraction > next_buffer as f64 {
            // try to resync buffer, e.g.: when the current run overlap with the next chunk
            let state = self.state.clone();
            let mut state = state.lock().unwrap();
            if let Some(start) = state.bar_start
                && next_buffer >= start
            {
                println!("{pos:.4} process buffer overlaps with next event buffer {stop_fraction} > {next_buffer}");
                self.sync_state_buffer(state.deref_mut(), next_buffer, context);
                self.send_events(start_fraction, stop_fraction, context);
            } else {
                println!(
                    "{pos:.4} ERROR: the next event buffer is not ready?! {next_buffer} {:?}",
                    state.bar_start
                );
            }
        }

        // Update status for the UI
        let mut dsp_state = self.dsp_state.write().unwrap();
        dsp_state.playing = playing;
        dsp_state.next = self.buffer_start_bar + BUFFER_LEN - cur_bar;
        dsp_state.remaining = self.buffer.len() - self.buffer_pos;
        ProcessStatus::KeepAlive
    }

    type BackgroundTask = PluguzuTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
        let editor_state = self.params.editor_state.clone();
        let dsp_state = self.dsp_state.clone();
        let state = self.state.clone();
        let current_bar = self.current_bar.clone();
        let evlocs_pos = self.evlocs_pos.clone();
        let evlocs = self.evlocs.clone();
        Box::new(move |task| {
            let mut state = state.lock().unwrap();
            let current_bar = current_bar.load(Ordering::Relaxed);
            match task {
                PluguzuTask::Parse(render) => {
                    let code = code.lock().unwrap();
                    println!("Parsing {}", code);
                    dsp_state.write().unwrap().error = state.parse(&code);
                    let start = current_bar + 1;
                    if render {
                        state.render(start, start + BUFFER_LEN);
                    }
                }
                PluguzuTask::Render(start) => {
                    nih_log!("{current_bar}: Rendering {}", start);
                    state.render(start, start + BUFFER_LEN);
                }
                PluguzuTask::UpdateHighlights => {
                    if editor_state.is_open() {
                        nih_log!("{current_bar}: Updating highlights");
                        highlights::update_locs(evlocs.clone(), &state.buffer, &state.locs);
                        evlocs_pos.store(0, Ordering::Relaxed);
                    }
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

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];
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
