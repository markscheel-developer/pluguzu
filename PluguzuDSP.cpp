// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "Pluguzu.hpp"

extern "C" void pluguzu_init(void);

void renderNextBar(pluguzu_state *pluguzuState, int32_t bar) {
  printf("Rendering next bar %d...\n", bar);
  pluguzuState->nextCount =
      pluguzuRender(pluguzuState->haskellState, bar, bar + 4,
                    (HsPtr)pluguzuState->nextEvents);
  pluguzuState->lastRenderedBar = bar;
}

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

PluguzuPlugin::PluguzuPlugin()
    : Plugin(0, 0, 1), // parameters, programs, states
      currentPos(0), currentCount(0) {
  pluguzuState.isPlaying = false;
  pluguzuState.nextBarStart = -1;
  pluguzuState.lastRenderedBar = 0;
  pluguzuState.haskellState = pluguzuNew();
  pluguzuState.nextCount = 0;
}

void PluguzuPlugin::initState(uint32_t index, State &state) {
  if (index != 0)
    return;

  state.hints = kStateIsHostWritable | kStateIsOnlyForUI;
  state.key = "text";
  state.label = "Text";
  state.description = "The Uzu code";
}

void PluguzuPlugin::setState(const char *key, const char *value) {
  if (std::strcmp(key, "text") != 0)
    return;

  printf("Updating code to --[%s]--\n", value);
  pluguzuParse(pluguzuState.haskellState, (HsPtr)value);
  const char *err = (const char *)pluguzuError(pluguzuState.haskellState);
  if (err) {
    printf("Parse error! %s for[\n%s\n]\n", err, value);
  } else {
    if (pluguzuState.lastRenderedBar == pluguzuState.nextBarStart)
      // The update happened before the next bar was rendered, so let's render
      // it now.
      pluguzuState.lastRenderedBar += 4;
    // otherwise we can override the previously rendered bar.
    renderNextBar(&pluguzuState, pluguzuState.lastRenderedBar);
  }
}

void PluguzuPlugin::run(const float ** /* inputs */, float ** /* outputs */,
                        uint32_t frames, const MidiEvent *midiEvents,
                        uint32_t midiEventCount) {
  // Forward midi input
  for (uint32_t idx = 0; idx < midiEventCount; idx++) {
    writeMidiEvent(midiEvents[idx]);
  }

  // Get the current host transport time position.
  // https://distrho.github.io/DPF/structTimePosition.html
  const TimePosition &timePos(getTimePosition());

  if (!timePos.playing) {
    // Ensure we traverse the whole buffer again on resume.
    currentPos = 0;

    // TODO: send noteOff events.
  }

  pluguzuState.isPlaying = timePos.playing;
  if (!timePos.playing || !timePos.bbt.valid)
    return;
  int32_t currentBar = timePos.bbt.bar - 1;

  // We are out of sync, we need a new render now
  if (currentBar < pluguzuState.nextBarStart ||
      currentBar >= pluguzuState.nextBarStart + 4) {
    printf("Maybe render off main thread?\n");
    renderNextBar(&pluguzuState, currentBar);
  }

  if (pluguzuState.lastRenderedBar != pluguzuState.nextBarStart &&
      pluguzuState.lastRenderedBar == currentBar) {
    printf("A new bar begin %d!\n", currentBar);

    // Send any left-over noteOff events
    // TODO: check if this is actually needed
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].velocity == 0)
        sendMidiEvent(currentPos);
    }
    syncEvents();
  }

  // There are no more events...
  if (currentPos >= currentCount) {
    return;
  }

  // Send the current midi events

  // Convert the current timePos tick into an absolute bar fraction
  // Note: the host time is given as a BarBeatTick,
  // so here we convert the current tick into a bar fraction to match tidal
  // cycle position.
  double beatFraction = timePos.bbt.tick / timePos.bbt.ticksPerBeat;
  double barFraction =
      (beatFraction + (timePos.bbt.beat - 1)) / timePos.bbt.beatsPerBar;
  double startFraction = currentBar + barFraction;

  // Then we need to know how long the current run will last in bar fraction.
  double runFraction;
  if (timePos.frame > 0) {
    // After the first run, we can use the timePos frame using a cross
    // product: frames        -> ? fraction timePos.frame -> startFraction
    runFraction = frames * startFraction / timePos.frame;
  } else {
    // Otherwise we can compute the current time based on the sample rate:
    double runTime = frames / getSampleRate();
    double beatPerSec = timePos.bbt.beatsPerMinute / 60;
    double barPerSec = beatPerSec / timePos.bbt.beatsPerBar;
    // Note: not sure why, but we need to divide by 4 again...
    runFraction = runTime / barPerSec / 4.0;
  }
  // Both runFraction calculation should be equivalent, but perhaps we should
  // only do the second one in case the timePos.frame is not monotonic.
  double stopFraction = startFraction + runFraction;

  // The goal of the above calculations is to get a stop time that will match
  // the next start time: The debug below will print:
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
    if (eventFraction + std::numeric_limits<double>::epsilon() < startFraction &&
        currentEvents[currentPos].velocity != 0) {
      // We somehow missed a tick, or perhaps the host fast forward to a new
      // bar.
      printf("Drop event! (current %.3f -- %.3f -- %.3f)\n", startFraction,
             eventFraction, stopFraction);
      continue;
    }
    printf("%.3f: Sending event %.3f %d\n", startFraction,
           currentEvents[currentPos].time, currentEvents[currentPos].velocity);
    sendMidiEvent(currentPos);
  }

  if (stopFraction > static_cast<double>(pluguzuState.nextBarStart + 4)) {
    // The current run overlapped with the end of the rendered bar, e.g. it
    // was 3.99 -- 4.01
    int32_t stopBar = std::lround(stopFraction);
    if (pluguzuState.lastRenderedBar != stopBar) {
      // The next bar is not ready.
      renderNextBar(&pluguzuState, stopBar);
      syncEvents();
    }
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].time > stopFraction)
        break;
      printf("%.3f: Sending event %.3f %d\n", startFraction,
             currentEvents[currentPos].time,
             currentEvents[currentPos].velocity);
      sendMidiEvent(currentPos);
    }
  }
}

void PluguzuPlugin::syncEvents() {
  // Copy the midi events from the pluguzuState
  // Note that there is a slight chance that the copy happens at the same
  // time of the update, Though we should be able to ensure the events
  // render happen way before, Otherwise, this could use a mutex.
  currentPos = 0;
  currentCount = pluguzuState.nextCount;
  memcpy(currentEvents, pluguzuState.nextEvents,
         currentCount * sizeof(midiEvent));
  pluguzuState.nextBarStart = pluguzuState.lastRenderedBar;
  /*
  // debug the loaded event
  for (uint32_t i = 0; i < currentCount; i++) {
    printf(" Next event at %.3f %d %d\n", currentEvents[i].time,
           currentEvents[i].pitch, currentEvents[i].velocity);
  }
  */
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
