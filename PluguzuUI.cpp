// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "DistrhoUI.hpp"

#include "DearImGuiColorTextEditor/TextEditor.h"

// The DSP plugin
#include "Pluguzu.hpp"

#include <fstream>
#include <iostream>

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

class PluguzuUI : public UI {
public:
  PluguzuUI() : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT, true), fScale(1.0) {
    setGeometryConstraints(80, 60);
    auto ldef = TextEditor::LanguageDefinition::HLSL();
    ldef.mName = "UZU";
    editor.SetLanguageDefinition(ldef);
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

  void onImGuiDisplay() override {
    if (pluguzuState == nullptr) {
      // The idle callback usually sets the pointer before the display,
      // in case it didn't, let's wait
      return;
    }
    ImGuiWindowFlags flags =
        ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize;
    ImGui::SetNextWindowPos(ImVec2(0, 0));
    ImGui::SetNextWindowSize(ImVec2(getWidth(), getHeight()));
    if (ImGui::Begin("TextEdit", nullptr, flags)) {

      if (pluguzuState == nullptr)
        ImGui::Text("Disconnected :(");
      else {
        ImGui::Text(
            "%s %d | %d events | %s  ", pluguzuState->isPlaying ? ">>=" : "|-|",
            pluguzuState->nextStartBar - pluguzuState->currentBar,
            pluguzuState->nextCount,
            pluguzuState->error.length() > 0 ? pluguzuState->error.buffer()
                                             : "OK");
        ImGui::SameLine();
        if (ImGui::Button("+")) {
          setScale(1.3);
        }
        ImGui::SameLine();
        if (ImGui::Button("-")) {
          setScale(0.7);
        }
      }
      editor.Render("TextEditor");
    }

    ImGui::End();
  }

  void setScale(float multiplier) {
    fScale = std::max((float)0.2, multiplier * fScale);
    ImGui::SetWindowFontScale(fScale);
    uint newWidth = std::max((uint)80, (uint)(multiplier * (float)getWidth()));
    uint newHeight = std::max((uint)60, (uint)(multiplier * (float)getHeight()));
    setSize(newWidth, newHeight);
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

  // ----------------------------------------------------------------------------------------------------------------
private:
  TextEditor editor;
  pluguzu_state *pluguzuState;
  bool textChangedState;
  bool textChanged = false;
  float fScale;

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
