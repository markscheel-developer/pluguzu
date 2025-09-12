// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor, egui,
    egui::{FontId, RichText, TextStyle, Vec2},
    resizable_window::ResizableWindow,
    EguiState,
};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::ops::DerefMut;
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU8};
use std::sync::{Arc, Mutex, RwLock};

use egui::{Color32, Shape, Stroke, StrokeKind};

pub mod binding;
mod presets;
use binding::PluguzuState;

mod highlights;

// Information to get midi event offset
pub struct ProcessMidi {
    start: f64,
    end: f64,
    frames_per_fraction: f64,
}

impl ProcessMidi {
    fn event_offset(&self, when: f64) -> u32 {
        ((when - self.start) * self.frames_per_fraction) as u32
    }
    fn do_send_note_off(
        &self,
        timing: u32,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        let mev = NoteEvent::NoteOff {
            timing,
            voice_id: None,
            channel: ev.channel,
            note,
            velocity: 0.,
        };
        println!("{:.4}: sending note off      {:?}", self.start, mev);
        context.send_event(mev);
    }
    fn send_note_off(
        &self,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        self.do_send_note_off(self.event_offset(ev.when), note, ev, context)
    }
}

// Info from the engine for the UI
struct PluguzuDSP {
    error: Option<String>,
    playing: bool,
    next: i32,
    remaining: usize,
}

pub struct Pluguzu {
    params: Arc<PluguzuParams>,
    state: Arc<Mutex<PluguzuState>>,

    dsp_state: Arc<RwLock<PluguzuDSP>>,

    note_offs: HashMap<u8, FutureNoteOff>,

    midi: ProcessMidi,
    current_pos: Arc<RwLock<u64>>,

    // Keep track of the current playing bar so that code update are queued to the next bar.
    current_bar: Arc<AtomicI32>,

    buffer: Vec<binding::MidiEvent>,
    buffer_pos: usize,
    buffer_start_bar: i32,

    evlocs: Arc<RwLock<Vec<highlights::EventLoc>>>,
    evlocs_pos: Arc<AtomicU32>,

    reset_requested: bool,
}

const BUFFER_LEN: i32 = 4;
const VERT_SPACE: f32 = 3.;

#[derive(Params)]
pub struct PluguzuParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[persist = "code"]
    code: Arc<Mutex<String>>,

    #[persist = "font-size"]
    font_size: Arc<AtomicU8>,
}

#[derive(Debug)]
struct FutureNoteOff {
    when: f64,
    channel: u8,
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
            current_bar: Arc::new(AtomicI32::new(0)),
            buffer_start_bar: -1,
            buffer_pos: 0,
            buffer: Vec::with_capacity(512),
            evlocs: Arc::new(RwLock::new(Vec::with_capacity(1024))),
            evlocs_pos: Arc::new(AtomicU32::new(0)),
            reset_requested: false,
            current_pos: Arc::new(RwLock::new(0)),
            midi: ProcessMidi {
                start: 0.,
                end: 0.,
                frames_per_fraction: 0.,
            },
        }
    }
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

pub enum PluguzuTask {
    Parse(bool),
    Render(i32),
    UpdateHighlights,
}

impl Pluguzu {
    fn sync_state_buffer(&mut self, cur_bar: i32, ctx: &impl ProcessContext<Self>) {
        let mut state = self.state.lock().unwrap();
        if let Some(start) = state.bar_start
            && cur_bar >= start
        {
            println!("Copying buffer data from {}", start);
            self.buffer.clear();
            self.buffer.extend_from_slice(&state.buffer);
            self.buffer_pos = 0;
            self.buffer_start_bar = start;
            state.bar_start = None;
            ctx.execute_background(PluguzuTask::UpdateHighlights);
            ctx.execute_background(PluguzuTask::Render(start + BUFFER_LEN))
        }
    }

    fn send_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        let midi = &self.midi;
        self.note_offs.retain(|note, ev| {
            if ev.when >= midi.start && ev.when < midi.end {
                midi.send_note_off(*note, ev, context);
                false
            } else {
                true
            }
        });
    }

    fn send_all_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        self.note_offs.retain(|note, ev| {
            self.midi.do_send_note_off(0, *note, ev, context);
            false
        });
    }

    fn send_event(&mut self, ev: &binding::MidiEvent, context: &mut impl ProcessContext<Self>) {
        let offset = if ev.start > self.midi.start {
            self.midi.event_offset(ev.start)
        } else {
            0
        };
        match NoteEvent::<()>::from_midi(offset, &ev.data) {
            Ok(mev) => {
                println!(
                    "{:.4}: sending event {:.4} {:?}",
                    self.midi.start, ev.start, mev
                );
                context.send_event(mev);
                if let NoteEvent::NoteOn { note, channel, .. } = mev {
                    if ev.stop < self.midi.end {
                        // This is a fast event, schedule the note-off right away
                        self.midi.send_note_off(
                            note,
                            &FutureNoteOff {
                                when: ev.stop,
                                channel,
                            },
                            context,
                        )
                    } else {
                        // Schedule the note-off for later
                        let prev = self.note_offs.insert(
                            note,
                            FutureNoteOff {
                                when: ev.stop,
                                channel,
                            },
                        );
                        if let Some(prev) = prev {
                            // Send any existing note-off
                            self.midi.send_note_off(note, &prev, context)
                        }
                    }
                }
            }
            Err(err) => println!("Couldn't decode {:?} {:?}", ev, err),
        }
    }
}

fn update_font_size(ctx: &egui::Context, font_size: u8) {
    ctx.all_styles_mut(|style| {
        // println!("All style: {:?}", style.text_styles);
        let font = FontId::monospace(font_size as f32);
        style.text_styles.insert(TextStyle::Button, font.clone());
        style.text_styles.insert(TextStyle::Body, font);
        // Make the UI more tight
        style.spacing.item_spacing.x = 4.;
    });
}

impl Plugin for Pluguzu {
    const NAME: &'static str = "Pluguzu";
    const VENDOR: &'static str = "Midirus";
    const URL: &'static str = "https://codeberg.org/TristanCacqueray/pluguzu";
    const EMAIL: &'static str = "tristan@midirus.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = PluguzuTask;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let egui_state = params.editor_state.clone();
        let code = params.code.clone();
        let font_size = params.font_size.clone();
        let initial_font_size = font_size.load(Ordering::Relaxed);

        let dsp_state = self.dsp_state.clone();
        let ui_view = Mutex::new(UIView::Editor);
        let preset_state = Mutex::new(PresetUI {
            cat: 0,
            selected: 0,
        });

        let presets = Arc::new(load_presets());

        let current_pos = self.current_pos.clone();
        let evlocs = self.evlocs.clone();
        let evlocs_pos = self.evlocs_pos.clone();

        let highlights = Mutex::new(std::collections::BTreeSet::<highlights::EventLoc>::new());

        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            move |egui_ctx, _| update_font_size(egui_ctx, initial_font_size),
            move |egui_ctx, _setter, _state| {
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(80.0, 60.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        // Main UI
                        let mut view = ui_view.lock().unwrap();
                        match *view {
                            UIView::Preset => preset_ui(
                                ui,
                                &mut view,
                                font_size.load(Ordering::Relaxed),
                                &presets,
                                preset_state.lock().unwrap().deref_mut(),
                                code.lock().unwrap().deref_mut(),
                            ),
                            UIView::Editor => {
                                let mut font_size_value = font_size.load(Ordering::Relaxed);
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Min),
                                    |ui| {
                                        if ui.button("+").clicked() {
                                            font_size_value = font_size_value.saturating_add(1);
                                            font_size.store(font_size_value, Ordering::Relaxed);
                                            update_font_size(egui_ctx, font_size_value);
                                        }
                                        if font_size_value > 4 && ui.button("-").clicked() {
                                            font_size_value = font_size_value.saturating_sub(1);
                                            font_size.store(font_size_value, Ordering::Relaxed);
                                            update_font_size(egui_ctx, font_size_value);
                                        }
                                        if ui.button("Load").clicked() {
                                            *view = UIView::Preset
                                        }
                                        let dsp_state = dsp_state.read().unwrap();
                                        let status = if dsp_state.playing { ">>=" } else { "|-|" };
                                        let next = dsp_state.next;
                                        let remaining = dsp_state.remaining;
                                        let error = dsp_state.error.as_deref().unwrap_or("OK");
                                        ui.label(RichText::new(format!(
                                            "{status} {next} | {remaining} events | {error}"
                                        )));
                                    },
                                );

                                let line_height = font_size_value as f32 + VERT_SPACE;
                                let avail_lines = (ui.available_height() / line_height).floor();

                                let output = CodeEditor::default()
                                    .id_source("code editor")
                                    .with_rows(avail_lines as usize)
                                    .with_fontsize(font_size_value as f32)
                                    .with_theme(ColorTheme::SONOKAI)
                                    .with_syntax(haskell())
                                    .with_numlines(true)
                                    .show(ui, code.lock().unwrap().deref_mut());

                                output.response.request_focus();

                                let painter = ui.painter_at(output.response.rect);
                                let ep = highlights::EditorPixels::new(
                                    output.galley_pos,
                                    &output.galley,
                                );

                                // Remove old highlights
                                let mut highlights = highlights.lock().unwrap();
                                let current_pos = current_pos.read().unwrap();
                                highlights.retain(|hl| current_pos.saturating_sub(hl.when) < 500);

                                // Add new highlights
                                let evlocs = evlocs.read().unwrap();
                                if dsp_state.read().unwrap().playing {
                                    let mut pos = evlocs_pos.load(Ordering::Relaxed) as usize;
                                    while pos < evlocs.len() {
                                        let evloc = &evlocs[pos];
                                        if *current_pos < evloc.when {
                                            break;
                                        }
                                        highlights.insert(evloc.clone());
                                        pos += 1;
                                    }
                                    evlocs_pos.store(pos as u32, Ordering::Relaxed);
                                }

                                let shapes = highlights.iter().map(|el| {
                                    let alpha = (egui::lerp(
                                        1.0..=0.3,
                                        (current_pos.saturating_sub(el.when) as f32) / 500.,
                                    ) * 255.) as u8;
                                    Shape::rect_stroke(
                                        ep.highlight_rect(el.col, el.row, el.len),
                                        5.,
                                        Stroke {
                                            width: 2.,
                                            color: Color32::from_rgba_unmultiplied(
                                                0xff, 0x6e, 0xc7, alpha,
                                            ),
                                        },
                                        StrokeKind::Middle,
                                    )
                                });
                                painter.extend(shapes);

                                if output.response.changed() {
                                    // TODO: sometime alt modifier get stuck after clicking, perhaps it could be manually removed to workaround this weird bug?
                                    // egui_ctx.input(|i| println!("HERE: {:?}", i.modifiers));
                                    async_executor.execute_background(PluguzuTask::Parse(true))
                                }
                            }
                        };
                    });
            },
        )
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
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
                    nih_log!("{current_bar}: Updating highlights");
                    highlights::update_locs(evlocs.clone(), &state.buffer, &state.locs);
                    evlocs_pos.store(0, Ordering::Relaxed);
                }
            }
        })
    }

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

        let transport = context.transport();
        let playing = transport.playing;
        let cur_bar = transport.bar_number().unwrap_or(0);
        self.current_bar.store(cur_bar, Ordering::Relaxed);

        if !playing || transport.preroll_active.unwrap_or(false) {
            self.dsp_state.write().unwrap().playing = false;

            // Advance the time for the highlights
            *self.current_pos.write().unwrap() +=
                ((self.midi.end - self.midi.start) * 1000.) as u64;

            self.send_all_note_offs(context);
            return ProcessStatus::Normal;
        }

        // Copy data
        self.sync_state_buffer(cur_bar, context);

        // Check that we are ready
        if cur_bar < self.buffer_start_bar || cur_bar >= self.buffer_start_bar + BUFFER_LEN {
            if !self.reset_requested {
                println!("We are out of sync, requesting a reset :/");
                self.buffer.clear();
                context.execute_background(PluguzuTask::Render(cur_bar));
                self.reset_requested = true;
            } else {
                println!("Waiting for the render...")
            }
            return ProcessStatus::KeepAlive;
        }
        self.reset_requested = false;

        // Compute current frame info
        let pos = transport.pos_beats().unwrap() / 4.0;
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
        self.midi.start = pos;
        self.midi.end = stop_fraction;
        if frames_per_fraction != self.midi.frames_per_fraction {
            println!("Frames per fraction changed to {}", frames_per_fraction);
            self.midi.frames_per_fraction = frames_per_fraction;
        }

        // Send any previously registered note-off.
        self.send_note_offs(context);

        // Send note-on
        let next_buffer = self.buffer_start_bar + BUFFER_LEN;
        while self.buffer_pos < self.buffer.len() {
            let ev = self.buffer[self.buffer_pos];
            if ev.start >= stop_fraction {
                // This event and the following oneare scheduled for later
                break;
            } else if start_fraction <= ev.start && ev.start < stop_fraction {
                // The event is due for this buffer
                self.send_event(&ev, context);
            } else if ev.stop > 0. && ev.stop <= stop_fraction {
                println!(
                    "{:.4} Missed event start, but it still make sense to send it now... {:.4} - {:.4} < {:.4}",
                    pos, ev.start, ev.stop, self.midi.end
                );
                self.send_event(&ev, context);
            } else if start_fraction > ev.start {
                // That's odd, it looks like we missed an event?!
                println!("{:.4} Skipped event {:.4}", pos, ev.start);
            }

            // Advance the event buffer position
            self.buffer_pos += 1;

            if self.buffer_pos == self.buffer.len() && stop_fraction > next_buffer as f64 {
                // try to resync buffer, e.g.: when the current run overlap with the next chunk
                self.sync_state_buffer(next_buffer, context)
            }
        }

        let mut dsp_state = self.dsp_state.write().unwrap();
        dsp_state.playing = playing;
        dsp_state.next = self.buffer_start_bar + BUFFER_LEN - cur_bar;
        dsp_state.remaining = self.buffer.len() - self.buffer_pos;
        ProcessStatus::KeepAlive
    }
}

enum UIView {
    Editor,
    Preset,
}

struct PresetUI {
    cat: usize,
    selected: usize,
}

type Presets = Vec<(String, Vec<Arc<presets::UzuProgram>>)>;

fn load_presets() -> Presets {
    let presets = presets::presets();
    let mut map = indexmap::IndexMap::with_capacity(presets.len());
    for preset in presets {
        let preset = Arc::new(preset);
        for tag in &preset.tags {
            map.entry(tag.to_string())
                .or_insert_with(Vec::new)
                .push(preset.clone())
        }
    }
    map.into_iter().collect()
}

fn preset_ui(
    ui: &mut egui::Ui,
    view: &mut UIView,
    font_size: u8,
    presets: &Presets,
    preset_state: &mut PresetUI,
    code: &mut String,
) {
    // Tag selector
    let prev_cat = preset_state.cat;
    let cat = &mut preset_state.cat;
    ui.horizontal(|ui| {
        for (pos, (tag, _)) in presets.iter().enumerate() {
            ui.selectable_value(cat, pos, tag);
        }
    });
    if prev_cat != *cat {
        preset_state.selected = 0;
    }

    ui.separator();
    // Preset selector
    let sel = &mut preset_state.selected;
    if let Some((_, presets)) = presets.get(*cat) {
        let line_height = font_size as f32 + VERT_SPACE;
        let avail_height = ui.available_height() - line_height * 2.0 + 10.;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .max_height(avail_height)
            .show(ui, |ui| {
                egui::Grid::new("my_grid")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (pos, preset) in presets.iter().enumerate() {
                            ui.selectable_value(sel, pos, preset.name);
                            ui.label(format!("{}", preset.code.len()));
                            ui.end_row();
                        }
                    });
                if let Some(preset) = presets.get(*sel)
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    *code = preset.code.to_string();
                    *view = UIView::Editor
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    *view = UIView::Editor
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    *sel = sel.saturating_sub(1)
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    *sel = sel.saturating_add(1)
                }
            });
        ui.horizontal(|ui| {
            if let Some(preset) = presets.get(*sel)
                && ui.button("Load").clicked()
            {
                *code = preset.code.to_string();
                *view = UIView::Editor
            };
            if ui.button("Cancel").clicked() {
                *view = UIView::Editor
            };
        });
    } else if ui.button("Oops").clicked() {
        *view = UIView::Editor
    }
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

pub fn haskell() -> Syntax {
    Syntax {
        language: "Haskell",
        case_sensitive: true,
        comment: "--",
        comment_multiline: ["{-", "-}"],
        hyperlinks: BTreeSet::from(["http"]),
        #[rustfmt::skip]
        keywords: BTreeSet::from([ // Update with grep -o "\"[a-zA-Z0-9]\+\"" /srv/codeberg.org/uzu/tidal/tidal-parse/src/Sound/Tidal/Parse.hs | tr '\n' ',' | sed 's/,/, /g'
            "True", "False", "literal", "major", "aug", "six", "sixNine", "major7", "major9", "add9", "major11",
            "add11", "major13", "add13", "dom7", "dom9", "dom11", "dom13", "sevenFlat5", "sevenSharp5", "sevenFlat9",
            "nine", "eleven", "thirteen", "minor", "diminished", "minorSharp5", "minor6", "minorSixNine", "minor7flat5",
            "minor7", "minor7sharp5", "minor7flat9", "minor7sharp9", "diminished7", "minor9", "minor11", "minor13",
            "one", "five", "sus2", "sus4", "sevenSus2", "sevenSus4", "nineSus4", "sevenFlat10", "nineSharp5",
            "minor9sharp5", "sevenSharp5flat9", "minor7sharp5flat9", "elevenSharp", "minor11sharp", "ur", "irand",
            "rand", "perlin", "silence", "silence", "sine", "saw", "isaw", "tri", "square", "cosine", "envEq", "envEqR",
            "envL", "envLR", "in", "layer", "spreadf", "ghost", "silent", "perlin2", "id", "brak", "rev", "palindrome",
            "stretch", "loopFirst", "degrade", "arpeggiate", "trigger", "rolled", "run", "listToPat", "choose",
            "cycleChoose", "stack", "fastcat", "fastCat", "slowcat", "slowCat", "cat", "randcat", "wrandcat", "s",
            "sound", "vowel", "cc", "nrpn", "cut", "nrpnn", "nrpnv", "ascii", "binary", "up", "n", "note", "midinote",
            "speed", "pan", "shape", "gain", "overgain", "overshape", "accelerate", "bandf", "bandq", "begin", "crush",
            "legato", "cutoff", "delayfeedback", "delaytime", "delay", "end", "hcutoff", "hresonance", "resonance",
            "loop", "coarse", "nudge", "amp", "velocity", "midibend", "midichan", "miditouch", "ccn", "ccv", "sseqs",
            "perlinWith", "inv", "wchoose", "timeCat", "timecat", "cF0", "perlin2With", "interlace", "overlay",
            "append", "slowAppend", "slowappend", "fastAppend", "fastappend", "const", "rotL", "rotR", "binaryN",
            "compress", "zoom", "compressTo", "samples", "fast", "fastGap", "density", "slow", "trunc", "densityGap",
            "sparsity", "linger", "segment", "discretise", "timeLoop", "swing", "ply", "iter", "slowstripe", "shuffle",
            "scramble", "repeatCycles", "stripe", "rot", "degradeBy", "unDegradeBy", "mask", "struct", "substruct",
            "distrib", "spaceOut", "chop", "striate", "gap", "randslice", "spin", "hurry", "loopAt", "rolledBy", "jux",
            "juxcut", "jux4", "sometimes", "often", "rarely", "almostNever", "almostAlways", "never", "always",
            "superimpose", "someCycles", "pF", "pN", "pI", "pS", "select", "squeeze", "scale", "toScale", "arp",
            "chooseBy", "wchooseBy", "sseq", "quantise", "cF", "range", "slice", "splice", "chew", "bite", "playFor",
            "swingBy", "wedge", "sew", "while", "euclid", "euclidInv", "degradeOverBy", "spread", "slowspread",
            "fastspread", "spreadChoose", "spreadr", "stitch", "striateBy", "off", "plyWith", "inside", "outside",
            "every", "plyWith", "chunk", "sometimesBy", "someCyclesBy", "plyWith", "juxBy", "fix", "unfix", "foldEvery",
            "within", "fit", "pickF", "weave", "weaveWith", "selectF", "lindenmayer", "rangex", "stutter", "euclidFull",
            "stut", "echo", "stutWith", "echoWith", "whenmod", "contrast"
        ]),
        types: BTreeSet::from([]),
        special: BTreeSet::from([]),
    }
}

nih_export_clap!(Pluguzu);
nih_export_vst3!(Pluguzu);
