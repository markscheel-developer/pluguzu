// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#ifndef DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED
#define DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED

#include "DistrhoPlugin.hpp"

// The Haskell state wrapper export
#include "Pluguzu_stub.h"

class PluguzuUI;

static const uint32_t maxMidiEvents = 512;

struct midiEvent {
  double time;
  int8_t pitch;
  int8_t velocity;
};

// The state shared between the DSP and the UI.
struct pluguzu_state {
  bool isPlaying;
  int32_t lastSyncedBar;
  int32_t lastRenderedBar;

  HsStablePtr haskellState;

  // next bar
  uint32_t nextCount;
  midiEvent nextEvents[maxMidiEvents];
};

START_NAMESPACE_DISTRHO

// --------------------------------------------------------------------------------------------------------------------

class PluguzuPlugin : public Plugin {
public:
  PluguzuPlugin();

protected:
  // ----------------------------------------------------------------------------------------------------------------
  // Information
  const char *getLabel() const noexcept override { return "Pluguzu"; }
  const char *getMaker() const noexcept override { return "tristanC"; }
  const char *getLicense() const noexcept override { return "GPLv3"; }
  uint32_t getVersion() const noexcept override { return d_version(1, 0, 0); }

  // ----------------------------------------------------------------------------------------------------------------
  // Init

  void initState(uint32_t index, State &state) override;
  void setState(const char *, const char *) override;

  // ----------------------------------------------------------------------------------------------------------------
  // Audio/MIDI Processing

  /**
     Run/process function for plugins without MIDI input.
     @note Some parameters might be null if there are no audio inputs or
     outputs.
   */
  void run(const float ** /* inputs */, float ** /* outputs */,
           uint32_t /* frames */, const MidiEvent * /* midiEvents */,
           uint32_t /* midiEventCount */) override;

  // ----------------------------------------------------------------------------------------------------------------
private:
  pluguzu_state pluguzuState;
  uint32_t currentPos;
  uint32_t currentCount;
  midiEvent currentEvents[maxMidiEvents];
  friend class PluguzuUI;
  void sendMidiEvent(uint32_t pos);

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuPlugin)
};

END_NAMESPACE_DISTRHO

#endif
