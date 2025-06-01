// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "DistrhoUI.hpp"

#include "DearImGuiColorTextEditor/TextEditor.h"

#include "dpf-widgets/generic/ResizeHandle.hpp"

// The DSP plugin
#include "Pluguzu.hpp"

#include <fstream>
#include <iostream>

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------
const TextEditor::LanguageDefinition& UZULang()
{
    static bool inited = false;
    static TextEditor::LanguageDefinition langDef;
    if (!inited)
    {
        static const char* const cppKeywords[] = {
          "case", "do", "if", "undefined"
        };
        for (auto& k : cppKeywords)
            langDef.mKeywords.insert(k);

        // Update with: grep -o "\"[a-zA-Z0-9]\+\"" /srv/github.com/tidalcycles/Tidal/tidal-parse/src/Sound/Tidal/Parse.hs | tr '\n' ', '
        static const char* const identifiers[] = {
"True","False","literal","major","aug","six","sixNine","major7","major9","add9","major11","add11","major13","add13","dom7","dom9","dom11","dom13","sevenFlat5","sevenSharp5","sevenFlat9","nine","eleven","thirteen","minor","diminished","minorSharp5","minor6","minorSixNine","minor7flat5","minor7","minor7sharp5","minor7flat9","minor7sharp9","diminished7","minor9","minor11","minor13","one","five","sus2","sus4","sevenSus2","sevenSus4","nineSus4","sevenFlat10","nineSharp5","minor9sharp5","sevenSharp5flat9","minor7sharp5flat9","elevenSharp","minor11sharp","ur","irand","rand","perlin","silence","silence","sine","saw","isaw","tri","square","cosine","envEq","envEqR","envL","envLR","in","layer","spreadf","ghost","silent","perlin2","id","brak","rev","palindrome","stretch","loopFirst","degrade","arpeggiate","trigger","rolled","run","listToPat","choose","cycleChoose","stack","fastcat","fastCat","slowcat","slowCat","cat","randcat","wrandcat","s","sound","vowel","cut","ascii","binary","up","n","note","speed","pan","shape","gain","overgain","overshape","accelerate","bandf","bandq","begin","crush","legato","cutoff","delayfeedback","delaytime","delay","end","hcutoff","hresonance","resonance","loop","coarse","nudge","sseqs","perlinWith","inv","wchoose","timeCat","timecat","cF0","perlin2With","interlace","overlay","append","slowAppend","slowappend","fastAppend","fastappend","const","rotL","rotR","binaryN","compress","zoom","compressTo","samples","fast","fastGap","density","slow","trunc","densityGap","sparsity","linger","segment","discretise","timeLoop","swing","ply","iter","slowstripe","shuffle","scramble","repeatCycles","stripe","rot","degradeBy","unDegradeBy","mask","struct","substruct","distrib","spaceOut","chop","striate","gap","randslice","spin","hurry","loopAt","rolledBy","jux","juxcut","jux4","sometimes","often","rarely","almostNever","almostAlways","never","always","superimpose","someCycles","pF","pN","pI","pS","select","squeeze","scale","toScale","arp","chooseBy","wchooseBy","sseq","quantise","cF","range","slice","splice","chew","bite","playFor","swingBy","wedge","sew","while","euclid","euclidInv","degradeOverBy","spread","slowspread","fastspread","spreadChoose","spreadr","stitch","striateBy","off","plyWith","inside","outside","every","plyWith","chunk","sometimesBy","someCyclesBy","plyWith","juxBy","fix","unfix","foldEvery","within","fit","pickF","weave","weaveWith","selectF","lindenmayer","rangex","stutter","euclidFull","stut","echo","stutWith","echoWith","whenmod","contrast"
        };
        for (auto& k : identifiers)
        {
          TextEditor::Identifier id;
            id.mDeclaration = "Built-in function";
            langDef.mIdentifiers.insert(std::make_pair(std::string(k), id));
        }

        auto hdef = TextEditor::LanguageDefinition::CPlusPlus();
        langDef.mTokenize = hdef.mTokenize;

        langDef.mCommentStart = "{-";
        langDef.mCommentEnd = "-}";
        langDef.mSingleLineComment = "--";

        langDef.mCaseSensitive = true;
        langDef.mAutoIndentation = true;

        langDef.mName = "UZU";

        inited = true;
    }
    return langDef;
}



class PluguzuUI : public UI {
public:
  PluguzuUI() : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT, true), fResizeHandle(this) {
    setGeometryConstraints(80, 60);
    editor.SetLanguageDefinition(UZULang());
    textChangedState = false;
    pluguzuState = nullptr;
  }

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // DSP/Plugin Callbacks
  void parameterChanged(uint32_t, float) override {}
  void stateChanged(const char *key, const char *value) override {
    if (std::strcmp(key, "uzu-code") != 0)
      return;

    textChangedState = true;
    editor.SetText(value);
  }

  void programLoaded(uint32_t index) override {
    stateChanged("uzu-code", uzu_programs[index].code);
  }

  void onImGuiDisplay() override {
    if (pluguzuState == nullptr) {
      // The idle callback usually sets the pointer before the display,
      // in case it didn't, let's wait
      return;
    }

    int offset = 0;
    ImGuiWindowFlags flags =
        ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize;
    ImGui::SetNextWindowPos(ImVec2(0, 0));
    ImGui::SetNextWindowSize(ImVec2(getWidth(), getHeight()));
    if (ImGui::Begin("TextEdit", nullptr, flags)) {
      if (pluguzuState == nullptr)
        ImGui::Text("Disconnected :(");
      else {
        // Font size controller
        if (ImGui::Button("+"))
          offset = 2;
        ImGui::SameLine();
        if (ImGui::Button(" - "))
          offset = -2;
        ImGui::SameLine();

        // Status line
        ImGui::Text(
            " %s %d | %d events | %s", pluguzuState->isPlaying ? ">>=" : "|-|",
            pluguzuState->nextStartBar - pluguzuState->currentBar,
            pluguzuState->nextCount,
            pluguzuState->error.length() > 0 ? pluguzuState->error.buffer()
                                             : "OK");
      }
      editor.Render("TextEditor");
    }

    ImGui::End();

    if (offset != 0) {
      updateFont(offset);
    }
  }

  void updateFont(int offset) {
    float newSize = std::min((float)42, std::max((float)2, ImGui::GetFontSize() + (float)offset));
    setFontSize(round(newSize));
  }

  // ----------------------------------------------------------------------------------------------------------------
  // UI Callbacks
  void uiIdle() override {
    if (pluguzuState == nullptr) {
      if (PluguzuPlugin *const dspPtr =
              (PluguzuPlugin *)getPluginInstancePointer()) {
        pluguzuState = &dspPtr->pluguzuState;
      }
    }

    // todo: display info about the current state, like errors or when next
    // cycle begin...

    if (editor.IsTextChangedSinceLastTime()) {
      if (textChangedState) {
        textChangedState = false;
      } else {
        textChanged = true;
      }
      return;
    }

    // wait for first idle with no more text changes
    if (textChanged) {
      auto str = editor.GetText();
      const char *txt = str.c_str();
      textChanged = false;
      setState("uzu-code", txt);
    }
  }

  // void onResize(const Widget::ResizeEvent& event) override  {
  //  printf("On resize go!\n");
  // }

  // ----------------------------------------------------------------------------------------------------------------
private:
  TextEditor editor;
  pluguzu_state *pluguzuState;
  bool textChangedState;
  bool textChanged = false;
  ResizeHandle fResizeHandle;

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
