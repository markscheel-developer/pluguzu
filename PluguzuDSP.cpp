// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#include "Pluguzu.hpp"

extern "C" {
void pluguzu_init(void);
void pluguzu_exit(void);
}

// doRenderNextBar updates the background buffer.
void doRenderNextBar(pluguzu_state *pluguzuState, int32_t bar) {
  pluguzuState->nextCount =
      pluguzuRender(pluguzuState->haskellState, bar, bar + 4,
                    (HsPtr)pluguzuState->nextEvents);
  if (pluguzuState->nextCount > 512) {
    // TODO: increase the buffer size?
    pluguzu_debug("Too many events rendered: %d", pluguzuState->nextCount);
    pluguzuState->nextCount = 512;
  }
  pluguzuState->nextStartBar = bar;
}

void renderNextBar(pluguzu_state *pluguzuState, int32_t bar) {
  pluguzu_debug("Rendering next bar %d (currentBar %d)...", bar,
                pluguzuState->currentBar);
  pluguzuState->mutex.lock();
  doRenderNextBar(pluguzuState, bar);
  pluguzuState->mutex.unlock();
}

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

PluguzuPlugin::PluguzuPlugin()
    : Plugin(0, sizeof(uzu_programs) / sizeof(uzu_programs[0]),
             1), // parameters, programs, states
      Runner("PluguzuRenderer"), lastStartFraction(0.0), lastStopFraction(0.0),
      currentPos(0), currentStartBar(-1), currentCount(0) {
  pluguzuState.isPlaying = false;
  pluguzuState.nextStartBar = 0;
  pluguzuState.haskellState = pluguzuNew();
  pluguzuState.nextCount = 0;
  pluguzuState.currentBar = 0;
}

void PluguzuPlugin::initProgramName(uint32_t index, String &name) {
  name = uzu_programs[index].name;
}

void PluguzuPlugin::loadProgram(uint32_t index) {
  setState("uzu-code", uzu_programs[index].code);
}

void PluguzuPlugin::initState(uint32_t index, State &state) {
  if (index != 0)
    return;

  state.hints =
      kStateIsHostWritable | kStateIsHostReadable | kStateIsOnlyForDSP;
  state.key = "uzu-code";
  state.label = "Text";
  state.description = "The Uzu code";
}

void PluguzuPlugin::setState(const char *key, const char *value) {
  if (std::strcmp(key, "uzu-code") != 0)
    return;

  pluguzu_debug("Updating code to --[%s]--", value);
  pluguzuState.mutex.lock();
  pluguzuParse(pluguzuState.haskellState, (HsPtr)value);
  const char *err = (const char *)pluguzuError(pluguzuState.haskellState);
  if (err) {
    // printf("Parse error! %s for[\n%s\n]\n", err, value);
    pluguzuState.error = err;
  } else {
    pluguzuState.error.clear();
    // We'll override the last background buffer.
    int32_t nextBar = pluguzuState.nextStartBar;
    if (pluguzuState.nextStartBar == currentStartBar) {
      // This is odd: the code was updated after the background buffer was
      // copied, but before it got updated. In that situation, we can create the
      // next background buffer.
      nextBar += 4;
    }
    doRenderNextBar(&pluguzuState, nextBar);

    // tell the host that the state change, unfortunately, that doesn't tell
    // REAPER that the project can be saved.
    // updateStateValue("uzu-code", value);
  }
  pluguzuState.mutex.unlock();
}

bool PluguzuPlugin::run() {
  if (currentStartBar == pluguzuState.nextStartBar) {
    pluguzu_debug("Rendering next bar in the background (currentStartBar %d)!",
                  currentStartBar);
    renderNextBar(&pluguzuState, pluguzuState.nextStartBar + 4);
  }
  return pluguzuState.isPlaying;
}

// Convert the current time position into an absolute bar fraction.
double getBarFraction(const TimePosition::BarBeatTick *bbt) {
  // The current position is the tick within the current beat.
  double beatFraction = bbt->tick / bbt->ticksPerBeat;
  double barFraction = (beatFraction + (bbt->beat - 1)) / bbt->beatsPerBar;
  return bbt->bar - 1 + barFraction;
}

// Compute how long a run time last in term of bar fraction.
double getRunFraction(double runTime, const TimePosition::BarBeatTick *bbt) {
  double beatPerSec = 60 / bbt->beatsPerMinute;
  double barLength = beatPerSec * bbt->beatsPerBar;
  return runTime / barLength;
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
    startRunner(500);

  int32_t currentBar = timePos.bbt.bar - 1;
  pluguzuState.currentBar = currentBar;

  // We are out of sync, we need a new render now
  if ((currentBar < currentStartBar || currentBar > currentStartBar + 4) &&
      currentBar != pluguzuState.nextStartBar) {
    pluguzu_debug("Rendering (currentBar: %d) in the foreground!", currentBar);
    renderNextBar(&pluguzuState, currentBar);
  }

  if (pluguzuState.nextStartBar != currentStartBar &&
      pluguzuState.nextStartBar == currentBar) {
    pluguzu_debug("A new bar begin (currentBar: %d, currentStartBar: %d, "
                  "nextStartBar: %d)!",
                  currentBar, currentStartBar, pluguzuState.nextStartBar);

    // TODO: requeue any left-over noteOff events
    // e.g. notes that ends after the buffer boundary must be taken care of...
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].velocity == 0)
        sendMidiEvent(0, 0, currentPos);
    }
    syncEvents();
  }

  // Send the current midi events

  // Note: the host time is given as a BarBeatTick,
  // so here we convert the current tick into a bar fraction to match tidal
  // cycle position.
  double startFraction = getBarFraction(&timePos.bbt);
  double runFraction = getRunFraction(frames / getSampleRate(), &timePos.bbt);
  double stopFraction = startFraction + runFraction;

  if (lastStopFraction > 0 &&
      std::abs(startFraction - lastStopFraction) > 0.0001) {
    pluguzu_debug(
        "Buffer start/end time don't align: previous was %f - %f, new is %f %f",
        lastStartFraction, lastStopFraction, startFraction, stopFraction);
  }
  lastStopFraction = stopFraction;

  if (startFraction < lastStartFraction) {
    pluguzu_debug(
        "Went back in time (startFraction: %f, lastStartFaction: %f)?!",
        startFraction, lastStartFraction);
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
  //   pluguzu_debug("Bar per sec: %f, frames: %d, start: %f, stop: %f",
  //   barPerSec, frames, startFraction, stopFraction);
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
      pluguzu_debug("Drop event! (%.3f -- %.3f -- %.3f)", startFraction,
                    eventFraction, stopFraction);
      continue;
    }
    pluguzu_debug("%.3f: Sending event %.3f %d", startFraction,
                  currentEvents[currentPos].time,
                  currentEvents[currentPos].velocity);
    sendMidiEvent(startFraction, framesPerFraction, currentPos);
  }

  if (stopFraction + std::numeric_limits<double>::epsilon() >
      static_cast<double>(currentStartBar + 4)) {
    // The current run overlapped with the end of the rendered bar, e.g. it
    // was 3.99 -- 4.01
    int32_t stopBar = std::floor(stopFraction);
    if (pluguzuState.nextStartBar != stopBar) {
      // The next bar is not ready.
      pluguzu_debug("Weird, the next bar is not ready!");
      renderNextBar(&pluguzuState, stopBar);
    } else {
      pluguzu_debug("Current run overlapped with next bar (stopFraction: %f, "
                    "stopBar: %d)...",
                    stopFraction, stopBar);
    }
    syncEvents();
    for (; currentPos < currentCount; currentPos++) {
      if (currentEvents[currentPos].time > stopFraction)
        break;
      pluguzu_debug("%.3f: Sending event %.3f %d", startFraction,
                    currentEvents[currentPos].time,
                    currentEvents[currentPos].velocity);
      sendMidiEvent(startFraction, framesPerFraction, currentPos);
    }
  }
}

void PluguzuPlugin::syncEvents() {
  // Copy the midi events from the pluguzuState
  currentPos = 0;
  pluguzu_debug(
      "Copying background buffer (nextStartBar: %d, currentStartBar: %d)",
      pluguzuState.nextStartBar, currentStartBar);
  pluguzuState.mutex.lock();
  currentCount = pluguzuState.nextCount;
  memcpy(currentEvents, pluguzuState.nextEvents,
         currentCount * sizeof(midiEvent));
  currentStartBar = pluguzuState.nextStartBar;
  pluguzuState.mutex.unlock();
  /*
  // debug the loaded event
  for (uint32_t i = 0; i < currentCount; i++) {
    pluguzu_debug(" Next event at %.3f %d %d\n", currentEvents[i].time,
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

static uint32_t count = 0;

Plugin *createPlugin() {
  if (!count) {
    pluguzu_init();
  }
  count++;
  return new PluguzuPlugin();
}

PluguzuPlugin::~PluguzuPlugin() {
  if (!--count) {
    pluguzu_exit();
  }
}

// --------------------------------------------------------------------------------------------------------------------

END_NAMESPACE_DISTRHO
