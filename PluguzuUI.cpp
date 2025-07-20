// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "DistrhoUI.hpp"

#include "DearImGuiColorTextEditor/TextEditor.h"

#include "dpf-widgets/generic/ResizeHandle.hpp"

// The DSP plugin
#include "Pluguzu.hpp"

#include <fstream>
#include <iostream>
#include <sys/stat.h>

void loadSoundMap(const char *fp, std::map<std::string, uint8_t> *map) {
  struct stat buffer;
  if (stat(fp, &buffer) == 0) {
    pluguzu_debug("%s: loading soundmap", fp);
    std::ifstream ConfFile(fp);
    std::string line;
    while (std::getline(ConfFile, line)) {
      auto sep = line.find(" = ");
      std::string key = line.substr(0, sep);
      std::string value = line.substr(sep + 3);
      (*map)[key] = std::stoi(value);
    }
  }
  if (map->empty()) {
    // Defaults from https://x42-plugins.com/x42/x42-avldrums
    (*map)["bd"] = 36;
    (*map)["sd"] = 37;
    (*map)["clap"] = 39;
    (*map)["lt"] = 41;
    (*map)["hh"] = 42;
    (*map)["oh"] = 46;
    (*map)["cb"] = 56;
  }
}

void saveSoundMap(const char *fp, std::map<std::string, uint8_t> *map) {
  try {
    std::ofstream ofile(fp);
    for (auto &sm : *map) {
      ofile << sm.first << " = " << std::to_string(sm.second) << "\n";
    }
  } catch (...) {
    pluguzu_debug("Couldn't save soundmap!");
  }
}

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
    inputSoundName[0] = 0;
    inputSoundValue = 0;
    setGeometryConstraints(80, 60);
    editor.SetLanguageDefinition(UZULang());
    editor.SetShowWhitespaces(false);
    textChangedState = false;
    // Ensure font is the right size.
    updateFont(0);
    pluguzuState = nullptr;
    for (int i = 0; i < uzu_presets_count; i++) {
      for (std::string tag : uzu_programs[i].tags) {
        selectedTags[tag] = true;
      }
    }
  }

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // DSP/Plugin Callbacks
  void parameterChanged(uint32_t, float) override {}
  void stateChanged(const char *key, const char *value) override {
    if (std::strcmp(key, "uzu-code") != 0)
      return;

    textChangedState = true;
    if (!strcmp(value, "")) {
      editor.SetText("-- Welcome to Pluguzu\n-- [ ] Click the Load button to "
                     "get started!\n");
      editor.SetCursorPosition(TextEditor::Coordinates(2, 0));
    } else {
      editor.SetText(value);
    }
  }

  void programLoaded(uint32_t index) override {
    stateChanged("uzu-code", uzu_programs[index].code);
  }

  void onImGuiDisplay() override {
    int offset = 0;
    ImGuiWindowFlags flags =
        ImGuiWindowFlags_NoTitleBar | ImGuiWindowFlags_NoResize;
    ImGui::SetNextWindowPos(ImVec2(0, 0));
    ImGui::SetNextWindowSize(ImVec2(getWidth(), getHeight()));
    if (ImGui::Begin("TextEdit", nullptr, flags)) {
      if (showPresetSelector) {
        displayPresetSelector();
      } else {
        if (pluguzuState == nullptr) {
          // The idle callback usually sets the pointer before the display,
          // in case it didn't, let's wait
          return;
        }
        if (showConfigurator) {
          displayConfigurator();
        } else {
          displayEditor(&offset);
        }
      }
    }
    ImGui::End();

    if (offset != 0) {
      updateFont(offset);
    }
  }

  void displayPresetSelector() {
    for (auto &tag : selectedTags) {
      ImGui::SameLine();
      ImGui::Checkbox(tag.first.c_str(), &tag.second);
    }
    if (ImGui::BeginListBox(
            "", ImVec2(-FLT_MIN, 17 * ImGui::GetTextLineHeightWithSpacing()))) {
      for (int i = 0; i < uzu_presets_count; i++) {
        bool is_visible = false;
        for (std::string tag : uzu_programs[i].tags) {
          if (selectedTags[tag]) {
            is_visible = true;
            break;
          }
        }
        if (!is_visible)
          continue;
        const bool is_selected = (selectedPreset == i);
        if (ImGui::Selectable(uzu_programs[i].name, is_selected))
          selectedPreset = i;
        // Set the initial focus when opening the combo (scrolling + keyboard
        // navigation focus)
        if (is_selected)
          ImGui::SetItemDefaultFocus();
      }
      ImGui::EndListBox();
    }

    if (ImGui::Button("Load")) {
      const char* code = uzu_programs[selectedPreset].code;
      editor.SetText(code);
      setState("uzu-code", code);
      showPresetSelector = false;
    }
    ImGui::SameLine();
    if (ImGui::Button("Append")) {
      const char* code = uzu_programs[selectedPreset].code;
      editor.SetText(editor.GetText() + "\n{-\n" + code + "-}");
      showPresetSelector = false;
    }
    ImGui::SameLine();
    if (ImGui::Button("Cancel"))
      showPresetSelector = false;
  }
  void clearInput() {
    inputSoundName[0] = 0;
    inputSoundValue = 0;
  }
  void displayConfigurator() {
    const char *to_delete = NULL;
    ImGui::TextUnformatted(pluguzuState->soundMapFile.c_str());
    if (ImGui::BeginListBox(
            "sound map",
            ImVec2(-FLT_MIN, 15 * ImGui::GetTextLineHeightWithSpacing()))) {
      for (auto &sm : pluguzuState->soundMap) {
        ImGui::PushID(sm.first.c_str());

        if (ImGui::Button("Remove")) {
          to_delete = sm.first.c_str();
          soundMapDirty = true;
        }
        ImGui::SameLine();

        int value = sm.second;
        ImGui::SetNextItemWidth(90);
        if (ImGui::InputInt("###pitch", &value)) {
          pluguzuState->soundMap[sm.first] = value;
          soundMapDirty = true;
        }
        ImGui::SameLine();
        ImGui::TextUnformatted(sm.first.c_str());
        ImGui::PopID();
      }
      ImGui::EndListBox();
    }
    if (to_delete) {
      pluguzuState->soundMap.erase(to_delete);
    }
    ImGui::SetNextItemWidth(90);
    ImGui::InputInt("###SoundVal", &inputSoundValue);
    ImGui::SameLine();
    ImGui::SetNextItemWidth(300);
    ImGui::InputText("New Sound", inputSoundName, IM_ARRAYSIZE(inputSoundName));

    if (inputSoundName[0] != 0 && inputSoundValue >= 0) {
      ImGui::SameLine();
      if (ImGui::Button("Add")) {
        pluguzuState->soundMap[inputSoundName] = inputSoundValue;
        soundMapDirty = true;
        clearInput();
      }
    }

    if (soundMapUnsaved || soundMapDirty) {
      if (ImGui::Button("Save")) {
        applySoundMap();
        saveSoundMap(pluguzuState->soundMapFile.c_str(),
                     &pluguzuState->soundMap);
        soundMapUnsaved = soundMapDirty = showConfigurator = false;
      }
      ImGui::SameLine();
    }

    if (soundMapDirty) {
      if (ImGui::Button("Apply")) {
        soundMapUnsaved = true;
        applySoundMap();
        soundMapDirty = showConfigurator = false;
      }
      ImGui::SameLine();
    }

    if (ImGui::Button(soundMapDirty ? "Revert" : "Close")) {
      if (soundMapDirty)
        pluguzuState->soundMap = soundMapBackup;
      soundMapDirty = showConfigurator = false;
    }
  }
  void applySoundMap() {
    for (auto &sm : pluguzuState->soundMap) {
      if (soundMapBackup.count(sm.first) == 0 || soundMapBackup[sm.first] != sm.second) {
        pluguzu_debug("Setting key: %s", sm.first.c_str());
        pluguzuSoundMap(pluguzuState->haskellState, (HsPtr)sm.first.c_str(),
                        sm.second);
      }
    }
  }
  void displayEditor(int *offset) {
    if (pluguzuState == nullptr)
      ImGui::Text("Disconnected :(");
    else {
      // Font size controller
      if (ImGui::Button("+"))
        *offset = 2;
      ImGui::SameLine();
      if (ImGui::Button(" - "))
        *offset = -2;
      ImGui::SameLine();
      if (ImGui::Button("Load"))
        showPresetSelector = true;
      ImGui::SameLine();
      if (ImGui::Button(soundMapUnsaved ? "Conf*" : "Conf")) {
        clearInput();
        soundMapBackup = pluguzuState->soundMap;
        showConfigurator = true;
      }
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
  bool showPresetSelector = false;
  bool showConfigurator = false;
  std::map<std::string, bool> selectedTags;
  int selectedPreset = 0;

  bool soundMapUnsaved = false;
  bool soundMapDirty = false;
  std::map<std::string, uint8_t> soundMapBackup;
  char inputSoundName[42];
  int inputSoundValue;
  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuUI)
};

// --------------------------------------------------------------------------------------------------------------------

UI *createUI() { return new PluguzuUI(); }

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
