// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0-or-later

#include "DistrhoUI.hpp"

#include "DearImGuiColorTextEditor/TextEditor.h"

// The DSP plugin
#include "Pluguzu.hpp"

#include <fstream>
#include <iostream>

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

class PluguzuUI : public UI {
  bool textChanged = false;

public:
  PluguzuUI() : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT) {
    const double scaleFactor = getScaleFactor();

    if (d_isEqual(scaleFactor, 1.0)) {
      setGeometryConstraints(DISTRHO_UI_DEFAULT_WIDTH,
                             DISTRHO_UI_DEFAULT_HEIGHT);
    } else {
      const uint width = DISTRHO_UI_DEFAULT_WIDTH * scaleFactor;
      const uint height = DISTRHO_UI_DEFAULT_HEIGHT * scaleFactor;
      setGeometryConstraints(width, height);
      setSize(width, height);
    }

    // showMenu();
    auto ldef = TextEditor::LanguageDefinition::HLSL();
    ldef.mName = "UZU";
    setLanguageDefinition(ldef);

    textChangedState = false;
  }

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // DSP/Plugin Callbacks
  void parameterChanged(uint32_t, float) override {}
  void stateChanged(const char *key, const char *value) override {
    if (std::strcmp(key, "text") != 0)
      return;

    textChangedState = true;
    setText(value);
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

    if (hasTextChangedSinceLastTime()) {
      if (textChangedState) {
        textChangedState = false;
      } else {
        textChanged = true;
      }
      return;
    }

    // wait for first idle with no more text changes
    if (textChanged) {
      auto str = getText();
      const char *txt = str.c_str();
      textChanged = false;
      setState("text", txt);
    }
  }

  // ----------------------------------------------------------------------------------------------------------------
private:
  pluguzu_state *pluguzuState;
  bool textChangedState;

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
