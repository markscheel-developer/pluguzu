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
const TextEditor::LanguageDefinition &UZULang() {
  static bool inited = false;
  static TextEditor::LanguageDefinition langDef;
  if (!inited) {
    static const char *const cppKeywords[] = {"case", "do", "if", "undefined"};
    for (auto &k : cppKeywords)
      langDef.mKeywords.insert(k);

    for (auto &k : tidal_identifiers) {
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
  PluguzuUI()
      : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT, true),
        fResizeHandle(this), fontSize(15) {
    setGeometryConstraints(80, 60);
    editor.SetLanguageDefinition(UZULang());
    textChangedState = false;
    // Ensure font is the right size.
    updateFont(0);
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
    fontSize = std::min(42, std::max(2, fontSize + offset));
    setFontSize(fontSize);
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
  int fontSize;

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
