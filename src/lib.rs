use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Vec2, resizable_window::ResizableWindow, EguiState};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::DerefMut;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

pub mod binding;
use binding::PluguzuState;

pub struct Pluguzu {
    params: Arc<PluguzuParams>,
    state: Arc<Mutex<PluguzuState>>,
    error: Arc<Mutex<Option<String>>>,

    note_offs: BTreeMap<u8, FutureNoteOff>,

    // Keep track of the current playing bar so that code update are queued to the next bar.
    current_bar: Arc<AtomicI32>,

    buffer: Vec<binding::MidiEvent>,
    buffer_pos: usize,
    buffer_start_bar: i32,

    reset_requested: bool,
}

#[derive(Params)]
pub struct PluguzuParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[persist = "code"]
    code: Arc<Mutex<String>>,
}

struct FutureNoteOff {
    when: f64,
    channel: u8,
}

impl Default for Pluguzu {
    fn default() -> Self {
        Self {
            params: Arc::new(PluguzuParams::default()),
            state: Arc::new(Mutex::new(PluguzuState::new(512))),
            error: Arc::new(Mutex::new(None)),
            note_offs: BTreeMap::new(),
            current_bar: Arc::new(AtomicI32::new(0)),
            buffer_start_bar: -1,
            buffer_pos: 0,
            buffer: Vec::with_capacity(512),
            reset_requested: false,
        }
    }
}

impl Default for PluguzuParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(300, 180),
            code: Arc::new(Mutex::new(
                "n \"c2*4\""
                    // "fast 2 $ n \"[c2 .. c3]\""
                    .into(),
            )),
        }
    }
}

pub enum PluguzuTask {
    Parse,
    Render(i32),
}

impl Pluguzu {
    fn sync_state_buffer(&mut self, cur_bar: i32, ctx: &impl ProcessContext<Self>) {
        let mut state = self.state.lock().unwrap();
        if let Some(start) = state.bar_start {
            if cur_bar >= start {
                println!("Copying buffer data from {}", start);
                self.buffer.extend_from_slice(&state.buffer);
                self.buffer_pos = 0;
                self.buffer_start_bar = start;
                state.bar_start = None;
                ctx.execute_background(PluguzuTask::Render(start + 4))
            }
        }
    }
}

impl Plugin for Pluguzu {
    const NAME: &'static str = "Pluguzu (nih)";
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
        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, _setter, _state| {
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(128.0, 128.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        let output = CodeEditor::default()
                            .id_source("code editor")
                            .with_rows(7)
                            .with_fontsize(14.0)
                            .with_theme(ColorTheme::SONOKAI)
                            .with_syntax(haskell())
                            .with_numlines(true)
                            .show(ui, code.lock().unwrap().deref_mut());
                        // TODO: request focus only once
                        output.response.request_focus();
                        if output.response.changed() {
                            // TODO: sometime alt modifier get stuck after clicking, perhaps it could be manually removed to workaround this weird bug?
                            // egui_ctx.input(|i| println!("HERE: {:?}", i.modifiers));
                            async_executor.execute_background(PluguzuTask::Parse)
                        }
                    });
            },
        )
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let code = self.params.code.clone();
        let error = self.error.clone();
        let state = self.state.clone();
        let current_bar = self.current_bar.clone();
        Box::new(move |task| {
            let mut state = state.lock().unwrap();
            match task {
                PluguzuTask::Parse => {
                    let code = code.lock().unwrap();
                    println!("Parsing {}", code);
                    *error.lock().unwrap() = state.parse(&code);
                    let start = current_bar.load(Ordering::Relaxed) + 1;
                    state.render(start, start + 4)
                }
                PluguzuTask::Render(start) => {
                    println!("Rendering {}", start);
                    state.render(start, start + 4)
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
        context.execute(PluguzuTask::Parse);
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
        let cur_bar = transport.bar_number().unwrap_or(0);
        self.current_bar.store(cur_bar, Ordering::Relaxed);

        // Copy data
        self.sync_state_buffer(cur_bar, context);

        // Check that we are ready
        if cur_bar < self.buffer_start_bar || cur_bar >= self.buffer_start_bar + 4 {
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

        let pos = transport.pos_beats().unwrap() / 4.0;

        let frames = buffer.samples() as f64;
        let beat_per_sec = 60. / transport.tempo.unwrap_or(120.);
        let bar_length = beat_per_sec * (transport.time_sig_numerator.unwrap_or(4) as f64);
        let run_time = frames / (transport.sample_rate as f64);
        let run_fraction = run_time / bar_length;
        let frames_per_fraction = frames / run_fraction;
        let stop_fraction = pos + run_fraction;

        // TODO: send note offs!

        while self.buffer_pos < self.buffer.len() {
            let ev = self.buffer[self.buffer_pos];
            if pos > ev.start {
                println!("{} Skipped event {}", pos, ev.start);
            } else if ev.start <= stop_fraction {
                let offset = (ev.start * frames_per_fraction) as u32;
                match NoteEvent::<()>::from_midi(offset, &ev.data) {
                    Ok(mev) => {
                        println!("{}: sending event {} {:?}", pos, ev.start, mev);
                        if let NoteEvent::NoteOn { note, channel, .. } = mev {
                            let prev = self.note_offs.insert(
                                note,
                                FutureNoteOff {
                                    when: ev.stop,
                                    channel,
                                },
                            );
                            // TODO: Send previous note off
                        }
                        context.send_event(mev);
                    }
                    Err(err) => println!("Couldn't decode {:?} {:?}", ev, err),
                }
                println!("now {}, ev {}", pos, ev.start);
            } else {
                break;
            }
            self.buffer_pos += 1;
        }

        ProcessStatus::KeepAlive
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
