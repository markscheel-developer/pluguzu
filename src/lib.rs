use egui_code_editor::{CodeEditor, ColorTheme, Syntax};
use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Vec2, resizable_window::ResizableWindow, EguiState};
use std::collections::BTreeSet;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};

pub mod binding;

pub struct Pluguzu {
    params: Arc<PluguzuParams>,
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

impl Default for Pluguzu {
    fn default() -> Self {
        Self {
            params: Arc::new(PluguzuParams::default()),
        }
    }
}

impl Default for PluguzuParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(300, 180),
            code: Arc::new(Mutex::new(String::new())),
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
    type BackgroundTask = ();

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
                            async_executor.execute_background(())
                        }
                    });
            },
        )
    }

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        // In the default implementation we can simply ignore the value
        let code = self.params.code.clone();
        Box::new(move |_| println!("Running in background for {}", code.lock().unwrap()))
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        true
    }

    fn process(
        &mut self,
        _buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Pass through midi events:
        while let Some(event) = context.next_event() {
            context.send_event(event)
        }
        ProcessStatus::Normal
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
