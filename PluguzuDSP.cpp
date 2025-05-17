// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0-or-later

#include "Pluguzu.hpp"

extern "C" void pluguzu_init(void);

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

PluguzuPlugin::PluguzuPlugin()
    : Plugin(0, 0, 1), // parameters, programs, states
      fState(nullptr) {}

void PluguzuPlugin::initState(uint32_t index, State &state) {
  if (index != 0)
    return;

  state.hints = kStateIsHostWritable | kStateIsOnlyForUI;
  state.key = "text";
  state.label = "Text";
}

void PluguzuPlugin::run(const float ** /* inputs */, float ** /* outputs */,
                        uint32_t /* frames */) {
  // printf("Frame! %d\n", frames);
  const TimePosition &timePos(getTimePosition());
  if (fState != nullptr) {
    fState->isPlaying = timePos.playing;
    if (timePos.playing && timePos.bbt.valid) {
      if (fState->nextBar == timePos.bbt.bar) {
        printf("New bar %d!\n", timePos.bbt.bar);
        // TODO: setup events...
      }
      fState->nextBar = timePos.bbt.bar + 1;
    }
  }
}

// --------------------------------------------------------------------------------------------------------------------

bool is_init = false;

Plugin *createPlugin() {
  if (!is_init) {
    pluguzu_init();
    is_init = true;
  }
  return new PluguzuPlugin();
}

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
