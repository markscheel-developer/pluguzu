// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "Pluguzu.hpp"

extern "C" void pluguzu_init(void);

void doRenderNextBar(pluguzu_state *pluguzuState, int32_t bar) {
  pluguzuState->nextCount =
      pluguzuRender(pluguzuState->haskellState, bar, bar + 4,
                    (HsPtr)pluguzuState->nextEvents);
  pluguzuState->nextStartBar = bar;
}

void renderNextBar(pluguzu_state *pluguzuState, int32_t bar) {
  printf("Rendering next bar %d...\n", bar);
  pluguzuState->mutex.lock();
  doRenderNextBar(pluguzuState, bar);
  pluguzuState->mutex.unlock();
}

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

PluguzuPlugin::PluguzuPlugin()
    : Plugin(0, 0, 1), // parameters, programs, states
      Runner("PluguzuRenderer"), lastStartFraction(0.0), currentPos(0),
      currentStartBar(-1), currentCount(0) {
  pluguzuState.isPlaying = false;
  pluguzuState.nextStartBar = 0;
  pluguzuState.haskellState = pluguzuNew();
  pluguzuState.nextCount = 0;
  pluguzuState.currentBar = 0;
}

void PluguzuPlugin::initState(uint32_t index, State &state) {
  if (index != 0)
    return;

  state.hints = kStateIsHostWritable | kStateIsOnlyForUI;
  state.key = "uzu-code";
  state.label = "Text";
  state.description = "The Uzu code";
}

void PluguzuPlugin::setState(const char *key, const char *value) {
  if (std::strcmp(key, "uzu-code") != 0)
    return;

  printf("Updating code to --[%s]--\n", value);
  pluguzuState.mutex.lock();
  pluguzuParse(pluguzuState.haskellState, (HsPtr)value);
  const char *err = (const char *)pluguzuError(pluguzuState.haskellState);
  if (err) {
    // printf("Parse error! %s for[\n%s\n]\n", err, value);
    pluguzuState.error = err;
  } else {
    pluguzuState.error.clear();
    if (pluguzuState.nextStartBar == currentStartBar)
      // The update happened before the next bar was rendered, so let's render
      // it now.
      pluguzuState.nextStartBar += 4;
    // otherwise we can override the previously rendered bar.
    doRenderNextBar(&pluguzuState, pluguzuState.nextStartBar);
  }
  pluguzuState.mutex.unlock();
}

bool PluguzuPlugin::run() {
  if (currentStartBar == pluguzuState.nextStartBar) {
    printf("Rendering next bar in the background!\n");
    renderNextBar(&pluguzuState, pluguzuState.nextStartBar + 4);
  }
  return pluguzuState.isPlaying;
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
  if (!isRunnerActive())
    startRunner(1000);

  int32_t currentBar = timePos.bbt.bar - 1;
  pluguzuState.currentBar = currentBar;

  // We are out of sync, we need a new render now
  if ((currentBar < currentStartBar || currentBar >= currentStartBar + 4) &&
      currentBar != pluguzuState.nextStartBar) {
    printf("We are out of sync and the current bar is not pre-rendered\n");
    renderNextBar(&pluguzuState, currentBar);
  }

  if (pluguzuState.nextStartBar != currentStartBar &&
      pluguzuState.nextStartBar == currentBar) {
    printf("A new bar begin %d!\n", currentBar);

    // Send any left-over noteOff events
    // TODO: check if this is actually needed
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].velocity == 0)
        sendMidiEvent(0, 0, currentPos);
    }
    syncEvents();
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
  double runTime = frames / getSampleRate();
  double beatPerSec = timePos.bbt.beatsPerMinute / 60;
  double barPerSec =
      beatPerSec * timePos.bbt.beatType / timePos.bbt.beatsPerBar;
  double runFraction = runTime / barPerSec;

  // So that we know when the current run stops.
  double stopFraction = startFraction + runFraction;

  if (startFraction < lastStartFraction) {
    printf("Went back in time?!\n");
    currentPos = 0;
  }
  lastStartFraction = startFraction;

  // As well as how long a barFraction is in term of frame for the event offset.
  double framesPerFraction = frames / runFraction;

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

  // if (currentPos < 2) {
  //   printf("Bar per sec: %f, frames: %d, start: %f, stop: %f\n", barPerSec,
  //   frames, startFraction, stopFraction);
  // }

  for (; currentPos < currentCount; currentPos++) {
    double eventFraction = currentEvents[currentPos].time;
    if (eventFraction > stopFraction) {
      // the event will be handled in a future run
      break;
    }
    if (eventFraction + std::numeric_limits<double>::epsilon() <
            startFraction &&
        currentEvents[currentPos].velocity != 0) {
      // We somehow missed a tick, or perhaps the host fast forward to a new
      // bar.
      printf("Drop event! (current %.3f -- %.3f -- %.3f)\n", startFraction,
             eventFraction, stopFraction);
      continue;
    }
    printf("%.3f: Sending event %.3f %d\n", startFraction,
           currentEvents[currentPos].time, currentEvents[currentPos].velocity);
    sendMidiEvent(startFraction, framesPerFraction, currentPos);
  }

  if (stopFraction > static_cast<double>(currentStartBar + 4)) {
    // The current run overlapped with the end of the rendered bar, e.g. it
    // was 3.99 -- 4.01
    int32_t stopBar = std::lround(stopFraction);
    if (pluguzuState.nextStartBar != stopBar) {
      // The next bar is not ready.
      printf("Weird, the next bar is not ready!\n");
      renderNextBar(&pluguzuState, stopBar);
    }
    syncEvents();
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].time > stopFraction)
        break;
      printf("%.3f: Sending event %.3f %d\n", startFraction,
             currentEvents[currentPos].time,
             currentEvents[currentPos].velocity);
      sendMidiEvent(startFraction, framesPerFraction, currentPos);
    }
  }
}

void PluguzuPlugin::syncEvents() {
  // Copy the midi events from the pluguzuState
  currentPos = 0;
  pluguzuState.mutex.lock();
  currentCount = pluguzuState.nextCount;
  memcpy(currentEvents, pluguzuState.nextEvents,
         currentCount * sizeof(midiEvent));
  currentStartBar = pluguzuState.nextStartBar;
  pluguzuState.mutex.unlock();
  /*
  // debug the loaded event
  for (uint32_t i = 0; i < currentCount; i++) {
    printf(" Next event at %.3f %d %d\n", currentEvents[i].time,
           currentEvents[i].pitch, currentEvents[i].velocity);
  }
  */
}

void PluguzuPlugin::sendMidiEvent(double startFraction,
                                  double framesPerFraction, uint32_t pos) {
  int8_t chan = 1;
  MidiEvent ev;
  uint32_t offset =
      (currentEvents[pos].time - startFraction) * framesPerFraction;
  ev.frame = offset;
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
