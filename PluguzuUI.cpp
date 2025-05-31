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
  PluguzuUI() : UI(DISTRHO_UI_DEFAULT_WIDTH, DISTRHO_UI_DEFAULT_HEIGHT, true), fScale(0.0) {
    setGeometryConstraints(80, 60);
    auto ldef = TextEditor::LanguageDefinition::HLSL();
    ldef.mName = "UZU";
    editor.SetLanguageDefinition(ldef);
    textChangedState = false;
    pluguzuState = nullptr;

    // Note: ideally, we would adjust the font size dynamically,
    // but calling this function outside of the constructor doesn't work (the character are rendered as white squares).
    // So we are forced to use 'SetWindowFontScale' for now, and hopefully we can revisit that workaround with the
    // upcoming new dear-imgui font api: https://github.com/ocornut/imgui/issues/6967#issuecomment-2870011366
    // Until then, we set the font size big and it will be scale down by half.
    setFontSize(42);

    // Moreover, we would like to adjust the font scale now, but calling it during the constructor causes a segfault :/
    // setScale(1.0)
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


    if (fScale < 0.1) {
      // Workaround because we can't call setScale in the constructor.
      fScale = 1.0;
      setScale(1.0);
    }

    ImGui::End();
  }


  // SetupFont allows changing the font and adjust the size, similar to what 'UI::setFontSize' does.
  // As explained in the class constructor note, this doesn't work dynamically unfortunately.
  /*
  void setupFont(float scale) {
    ImGuiIO& io = ImGui::GetIO();
    io.Fonts->Flags |= ImFontAtlasFlags_NoPowerOfTwoHeight;
    io.Fonts->Clear();

    ImFontConfig fc;
    fc.FontDataOwnedByAtlas = false;
    fc.OversampleV = fc.OversampleH = 1;
    fc.PixelSnapH = true;

    float sz = round(scale * 22);
    fc.SizePixels = sz;
    printf("Loading font at %f\n", sz);
    // const char* ttf_compressed_base85 = proggy_clean_ttf_compressed_data_base85;
    // io.FontDefault = io.Fonts->AddFontFromMemoryCompressedBase85TTF(ttf_compressed_base85, sz, &fc);
    io.FontDefault = io.Fonts->AddFontFromFileTTF("/nix/store/4a0vxcwkvndszpyzf3pn7w2a2mlw50y5-iosevka-comfy-wide-2.1.0/share/fonts/truetype/iosevka-comfy-wide-normalregularupright.ttf", sz, &fc);
    io.Fonts->Build();
  }
  */

  void setScale(float multiplier) {
    fScale = std::max((float)0.2, multiplier * fScale);

    // Here we scale down by half because the initial font size is big to avoid blurryness when increasing...
    double scaleFactor = getScaleFactor();
    double fontScale = fScale * 0.5 * scaleFactor;
    printf("Adjusting font scale: %f (factor %f)\n", fontScale, scaleFactor);
    ImGui::SetWindowFontScale(fontScale);

    // Update the window size too.
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

  // void onResize(const Widget::ResizeEvent& event) override  {
  //  printf("On resize go!\n");
  // }

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
