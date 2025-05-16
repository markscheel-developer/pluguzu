// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0-or-later

#include "DistrhoUI.hpp"

#include "DearImGuiColorTextEditor/TextEditor.h"

#include "Pluguzu.hpp"

#include "Pluguzu_stub.h"

#include <fstream>

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

class PluguzuUI : public UI {
  bool textChanged = false;

public:
  /**
     UI class constructor.
     The UI should be initialized to a default state that matches the plugin
     side.
   */
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

    fState.isPlaying = false;
  }

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // DSP/Plugin Callbacks

  /**
     A parameter has changed on the plugin side.@n
     This is called by the host to inform the UI about parameter changes.
   */
  void parameterChanged(uint32_t, float) override {}

  /**
     A state has changed on the plugin side.@n
     This is called by the host to inform the UI about state changes.
   */
  void stateChanged(const char *key, const char *value) override {
    if (std::strcmp(key, "text") != 0)
      return;

    setText(value);
  }

  // ----------------------------------------------------------------------------------------------------------------
  // UI Callbacks

  /**
     UI idle function, called to give idle time to the plugin UI directly from
     the host. This is called right after OS event handling and Window idle
     events (within the same cycle). There are no guarantees in terms of timing.
   */
  void uiIdle() override {
    if (PluguzuPlugin *const dspPtr =
            (PluguzuPlugin *)getPluginInstancePointer()) {
      if (dspPtr->fState == nullptr) {
        dspPtr->fState = &fState;
      }
    }

    if (hasTextChangedSinceLastTime()) {
      textChanged = true;
      return;
    }

    // wait for first idle with no more text changes
    if (textChanged) {
      printf("Is playing? %d\n", fState.isPlaying);
      someFunc();
      textChanged = false;
      setState("text", getText().c_str());
    }
  }

  /**
     Window file selected function, called when a path is selected by the user,
     as triggered by openFileBrowser(). This function is for plugin UIs to be
     able to override Window::onFileSelected(const char*).

     This action happens after the user confirms the action, so the file browser
     dialog will be closed at this point. The default implementation does
     nothing.

     If you need to use files as plugin state, please setup and use states with
     kStateIsFilenamePath instead.
   */
  void uiFileBrowserSelected(const char *const filename) override {
    // TODO check if this can be null, add to docs if that can happen
    if (filename == nullptr)
      return;

    std::ifstream filestream(filename);

    if (filestream.good()) {
      const std::string text((std::istreambuf_iterator<char>(filestream)),
                             std::istreambuf_iterator<char>());
      setText(text);
      setState("text", text.c_str());
    }
  }

  // ----------------------------------------------------------------------------------------------------------------
private:
  pluguzuState fState;

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
