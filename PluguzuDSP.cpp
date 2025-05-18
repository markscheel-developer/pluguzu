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
                        uint32_t frames, const MidiEvent* midiEvents, uint32_t midiEventCount) {
  // Forward midi input
  for (uint32_t idx = 0; idx < midiEventCount; idx++) {
      writeMidiEvent(midiEvents[idx]);
  }

  // Get the current host transport time position.
  // https://distrho.github.io/DPF/structTimePosition.html
  const TimePosition &timePos(getTimePosition());

  // TODO: setup the state from the DSP
  if (fState != nullptr) {
    fState->isPlaying = timePos.playing;
    if (timePos.playing && timePos.bbt.valid) {
      if (fState->lastRenderedBar == timePos.bbt.bar - 1) {
        printf("A new bar begin %d!\n", timePos.bbt.bar);

        // Send left-over noteOff events
        for (; currentPos < currentCount; currentPos++) {
          if (currentEvents[currentPos].velocity == 0)
            sendMidiEvent(currentPos);
        }

        // Copy the midi events from the pluguzuState
        // Note that there is a slight chance that the copy happens at the same
        // time of the update, Though we should be able to ensure the events
        // render happen way before, Otherwise, this could use a mutex.
        currentPos = 0;
        currentCount = fState->nextCount;
        memcpy(currentEvents, fState->nextEvents,
               currentCount * sizeof(midiEvent));
        for (uint32_t i = 0; i < currentCount; i++) {
          printf(" Next event at %.3f %d %d\n", currentEvents[i].time,
                 currentEvents[i].pitch, currentEvents[i].velocity);
        }
        fState->lastSyncBar = fState->lastRenderedBar;
        fState->lastRenderedBar = -1;
      }

      // Send the current midi events

      // Convert the current timePos tick into an absolute bar fraction
      // Note: the host time is given as a BarBeatTick,
      // so here we convert the current tick into a bar fraction to match tidal cycle position.
      double beatFraction = timePos.bbt.tick / timePos.bbt.ticksPerBeat;
      double barFraction =
          (beatFraction + (timePos.bbt.beat - 1)) / timePos.bbt.beatsPerBar;
      double startFraction = (timePos.bbt.bar - 1) + barFraction;

      // Then we need to know how long the current run will last in bar fraction.
      double runFraction;
      if (timePos.frame > 0) {
        // After the first run, we can use the timePos frame using a cross product:
        // frames        -> ? fraction
        // timePos.frame -> startFraction
        runFraction = frames * startFraction / timePos.frame;
      } else {
        // Otherwise we can compute the current time based on the sample rate:
        double runTime = frames / getSampleRate();
        double beatPerSec = timePos.bbt.beatsPerMinute / 60;
        double barPerSec = beatPerSec / timePos.bbt.beatsPerBar;
        // Note: not sure why, but we need to divide by 4 again...
        runFraction = runTime / barPerSec / 4.0;
      }
      double stopFraction = startFraction + runFraction;

      // The goal of the above function is to get a stop time that will match the next start time:
      // The debug below will print:
      //    Start/stop: 0.000000 -- 0.010667
      //    Start/stop: 0.010667 -- 0.021333
      //    Start/stop: 0.021333 -- 0.032000
      //    Start/stop: 0.032000 -- 0.042667
      //    Start/stop: 0.042667 -- 0.053333
      //    Start/stop: 0.053333 -- 0.064000
      //    Start/stop: 0.064000 -- 0.074667
      // printf("Start/stop: %.6f -- %.6f\n", startFraction, stopFraction);

      for (; currentPos < currentCount; currentPos++) {
        double eventFraction = currentEvents[currentPos].time;
        if (eventFraction > stopFraction) {
          // the event will be handled in a future run
          break;
        }
        if (eventFraction < startFraction && currentEvents[currentPos].velocity != 0) {
          // We somehow missed a tick, or perhaps the host fast forward to a new bar.
          printf("Drop event! (current %.3f -- %.3f -- %.3f)\n", startFraction, eventFraction, stopFraction);
          continue;
        }
        printf("%.3f: Sending event %.3f\n", startFraction, currentEvents[currentPos].time);
        sendMidiEvent(currentPos);
      }
    }
    if (!timePos.playing) {
      currentPos = 0;
    }
  }
}

void PluguzuPlugin::sendMidiEvent(uint32_t pos) {
  int8_t chan = 1;
  MidiEvent ev;
  ev.frame = 0; // TODO: calculate offset
  ev.size = 3;
  ev.data[0] = (currentEvents[pos].velocity != 0 ? 0x90 : 0x80) | chan;
  ev.data[1] = currentEvents[pos].pitch;
  ev.data[2] = currentEvents[pos].velocity;
  ev.dataExt = NULL;
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
