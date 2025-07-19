// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#ifndef DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED
#define DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED

#include "DistrhoPlugin.hpp"

#include "dpf/distrho/extra/Mutex.hpp"
#include "dpf/distrho/extra/Runner.hpp"

// The Haskell state wrapper export
#include "Pluguzu_stub.h"

#include <string>
#include <vector>

// Set to 0 to disable printf debug
#define PLUGUZU_DEBUG 1

// The default programs
struct uzu_program {
  const char *name;
  const char *code;
  const std::vector<std::string> tags;
};

const uzu_program uzu_programs[] = {
    {"Default", "-- welcome", {"comment"}},
    {"Scales", "n \"[0 .. 24]\"", {"demo"}},
};
const int uzu_presets_count = sizeof(uzu_programs) / sizeof(uzu_programs[0]);
// --

class PluguzuUI;

static const uint32_t maxMidiEvents = 512;

struct midiEvent {
  // Time is noted as a bar fraction, independent of the BPM
  double time;
  uint8_t d0;
  uint8_t d1;
  uint8_t d2;
};

// The state shared between the DSP and the UI.
struct pluguzu_state {
  bool isPlaying;
  int32_t currentBar;

  HsStablePtr haskellState;

  String error;

  // The next bar pre-rendered, ready to be copied by the run callback
  Mutex mutex;
  int32_t nextStartBar;
  uint32_t nextCount;
  midiEvent nextEvents[maxMidiEvents];
};

#define pluguzu_debug(fmt, ...)                                                \
  do {                                                                         \
    if (PLUGUZU_DEBUG) {                                                       \
      printf("[pluguzu] " fmt "\n", ##__VA_ARGS__);                            \
    }                                                                          \
  } while (0)

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

class PluguzuPlugin : public Plugin, public Runner {
public:
  PluguzuPlugin();
  ~PluguzuPlugin() override;

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // Information
  const char *getLabel() const noexcept override { return "Pluguzu"; }
  const char *getMaker() const noexcept override { return "tristanC"; }
  const char *getLicense() const noexcept override { return "GPLv3"; }
  uint32_t getVersion() const noexcept override { return d_version(1, 0, 0); }

  // ----------------------------------------------------------------------------------------------------------------
  // Init

  void initState(uint32_t index, State &state) override;
  void setState(const char *, const char *) override;

  void initProgramName(uint32_t, String &) override;
  void loadProgram(uint32_t) override;

  // ----------------------------------------------------------------------------------------------------------------
  // Audio/MIDI Processing

  /**
     Run/process function for plugins without MIDI input.
     @note Some parameters might be null if there are no audio inputs or
     outputs.
   */
  void run(const float ** /* inputs */, float ** /* outputs */,
           uint32_t /* frames */, const MidiEvent * /* midiEvents */,
           uint32_t /* midiEventCount */) override;

  bool run() override;

  // ----------------------------------------------------------------------------------------------------------------
private:
  pluguzu_state pluguzuState;
  double lastStartFraction;
  double lastStopFraction;
  double lastTick;
  uint32_t currentPos;
  int32_t currentStartBar;
  uint32_t currentCount;
  midiEvent currentEvents[maxMidiEvents];
  friend class PluguzuUI;
  void sendMidiEvent(double barFraction, double framesPerFraction,
                     uint32_t pos);

  void syncEvents();

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuPlugin)
};

END_NAMESPACE_DISTRHO

// Update with: grep -o "\"[a-zA-Z0-9]\+\""
// /srv/github.com/tidalcycles/Tidal/tidal-parse/src/Sound/Tidal/Parse.hs | tr
// '\n' ', '
static const char *const tidal_identifiers[] = {"True",
                                                "False",
                                                "literal",
                                                "major",
                                                "aug",
                                                "six",
                                                "sixNine",
                                                "major7",
                                                "major9",
                                                "add9",
                                                "major11",
                                                "add11",
                                                "major13",
                                                "add13",
                                                "dom7",
                                                "dom9",
                                                "dom11",
                                                "dom13",
                                                "sevenFlat5",
                                                "sevenSharp5",
                                                "sevenFlat9",
                                                "nine",
                                                "eleven",
                                                "thirteen",
                                                "minor",
                                                "diminished",
                                                "minorSharp5",
                                                "minor6",
                                                "minorSixNine",
                                                "minor7flat5",
                                                "minor7",
                                                "minor7sharp5",
                                                "minor7flat9",
                                                "minor7sharp9",
                                                "diminished7",
                                                "minor9",
                                                "minor11",
                                                "minor13",
                                                "one",
                                                "five",
                                                "sus2",
                                                "sus4",
                                                "sevenSus2",
                                                "sevenSus4",
                                                "nineSus4",
                                                "sevenFlat10",
                                                "nineSharp5",
                                                "minor9sharp5",
                                                "sevenSharp5flat9",
                                                "minor7sharp5flat9",
                                                "elevenSharp",
                                                "minor11sharp",
                                                "ur",
                                                "irand",
                                                "rand",
                                                "perlin",
                                                "silence",
                                                "silence",
                                                "sine",
                                                "saw",
                                                "isaw",
                                                "tri",
                                                "square",
                                                "cosine",
                                                "envEq",
                                                "envEqR",
                                                "envL",
                                                "envLR",
                                                "in",
                                                "layer",
                                                "spreadf",
                                                "ghost",
                                                "silent",
                                                "perlin2",
                                                "id",
                                                "brak",
                                                "rev",
                                                "palindrome",
                                                "stretch",
                                                "loopFirst",
                                                "degrade",
                                                "arpeggiate",
                                                "trigger",
                                                "rolled",
                                                "run",
                                                "listToPat",
                                                "choose",
                                                "cycleChoose",
                                                "stack",
                                                "fastcat",
                                                "fastCat",
                                                "slowcat",
                                                "slowCat",
                                                "cat",
                                                "randcat",
                                                "wrandcat",
                                                "s",
                                                "sound",
                                                "vowel",
                                                "cut",
                                                "ascii",
                                                "binary",
                                                "up",
                                                "n",
                                                "note",
                                                "speed",
                                                "pan",
                                                "shape",
                                                "gain",
                                                "overgain",
                                                "overshape",
                                                "accelerate",
                                                "bandf",
                                                "bandq",
                                                "begin",
                                                "crush",
                                                "legato",
                                                "cutoff",
                                                "delayfeedback",
                                                "delaytime",
                                                "delay",
                                                "end",
                                                "hcutoff",
                                                "hresonance",
                                                "resonance",
                                                "loop",
                                                "coarse",
                                                "nudge",
                                                "sseqs",
                                                "perlinWith",
                                                "inv",
                                                "wchoose",
                                                "timeCat",
                                                "timecat",
                                                "cF0",
                                                "perlin2With",
                                                "interlace",
                                                "overlay",
                                                "append",
                                                "slowAppend",
                                                "slowappend",
                                                "fastAppend",
                                                "fastappend",
                                                "const",
                                                "rotL",
                                                "rotR",
                                                "binaryN",
                                                "compress",
                                                "zoom",
                                                "compressTo",
                                                "samples",
                                                "fast",
                                                "fastGap",
                                                "density",
                                                "slow",
                                                "trunc",
                                                "densityGap",
                                                "sparsity",
                                                "linger",
                                                "segment",
                                                "discretise",
                                                "timeLoop",
                                                "swing",
                                                "ply",
                                                "iter",
                                                "slowstripe",
                                                "shuffle",
                                                "scramble",
                                                "repeatCycles",
                                                "stripe",
                                                "rot",
                                                "degradeBy",
                                                "unDegradeBy",
                                                "mask",
                                                "struct",
                                                "substruct",
                                                "distrib",
                                                "spaceOut",
                                                "chop",
                                                "striate",
                                                "gap",
                                                "randslice",
                                                "spin",
                                                "hurry",
                                                "loopAt",
                                                "rolledBy",
                                                "jux",
                                                "juxcut",
                                                "jux4",
                                                "sometimes",
                                                "often",
                                                "rarely",
                                                "almostNever",
                                                "almostAlways",
                                                "never",
                                                "always",
                                                "superimpose",
                                                "someCycles",
                                                "pF",
                                                "pN",
                                                "pI",
                                                "pS",
                                                "select",
                                                "squeeze",
                                                "scale",
                                                "toScale",
                                                "arp",
                                                "chooseBy",
                                                "wchooseBy",
                                                "sseq",
                                                "quantise",
                                                "cF",
                                                "range",
                                                "slice",
                                                "splice",
                                                "chew",
                                                "bite",
                                                "playFor",
                                                "swingBy",
                                                "wedge",
                                                "sew",
                                                "while",
                                                "euclid",
                                                "euclidInv",
                                                "degradeOverBy",
                                                "spread",
                                                "slowspread",
                                                "fastspread",
                                                "spreadChoose",
                                                "spreadr",
                                                "stitch",
                                                "striateBy",
                                                "off",
                                                "plyWith",
                                                "inside",
                                                "outside",
                                                "every",
                                                "plyWith",
                                                "chunk",
                                                "sometimesBy",
                                                "someCyclesBy",
                                                "plyWith",
                                                "juxBy",
                                                "fix",
                                                "unfix",
                                                "foldEvery",
                                                "within",
                                                "fit",
                                                "pickF",
                                                "weave",
                                                "weaveWith",
                                                "selectF",
                                                "lindenmayer",
                                                "rangex",
                                                "stutter",
                                                "euclidFull",
                                                "stut",
                                                "echo",
                                                "stutWith",
                                                "echoWith",
                                                "whenmod",
                                                "contrast"};

#endif
