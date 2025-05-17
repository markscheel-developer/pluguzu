// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0-or-later

#include "Pluguzu.hpp"

extern "C" void pluguzu_init(void);

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

PluguzuPlugin::PluguzuPlugin()
    : Plugin(0, 0, 1), // parameters, programs, states
      fState(nullptr), currentPos(0), currentCount(0) {}

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

        // Send left-over events
        for (; currentPos < currentCount; currentPos++)
          sendMidiEvent(currentPos);

        // Copy the midi events from the pluguzuState
        currentPos = 0;
        currentCount = fState->nextCount;
        memcpy(currentMidisPos, fState->nextMidisPos,
               currentCount * sizeof(double));
        memcpy(currentMidisPitch, fState->nextMidisPitch,
               currentCount * sizeof(int8_t));
        memcpy(currentMidisVelocity, fState->nextMidisVelocity,
               currentCount * sizeof(int8_t));
      }
      fState->nextBar = timePos.bbt.bar + 1;

      // Send the current midi events
      double beatFraction = timePos.bbt.tick / timePos.bbt.ticksPerBeat;
      double barFraction =
          (beatFraction + (timePos.bbt.beat - 1)) / timePos.bbt.beatsPerBar;
      for (; currentPos < currentCount; currentPos++) {
        if (currentMidisPos[currentPos] > barFraction) {
          break;
        }
        sendMidiEvent(currentPos);
      }
    }
  }
}

void PluguzuPlugin::sendMidiEvent(uint32_t pos) {
  int8_t chan = 1;
  MidiEvent ev;
  ev.frame = 0; // TODO: calculate offset
  ev.size = 3;
  ev.data[0] = (currentMidisVelocity[pos] != 0 ? 0x90 : 0x80) | chan;
  ev.data[1] = currentMidisPitch[pos];
  ev.data[2] = currentMidisVelocity[pos];
  ev.dataExt = NULL;
  printf("Sending event!\n");
  writeMidiEvent(ev);
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
