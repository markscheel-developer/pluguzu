// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

#ifndef DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED
#define DISTRHO_PLUGIN_PLUGUZU_HPP_INCLUDED

#include "DistrhoPlugin.hpp"

class PluguzuUI;

static const uint32_t maxMidiEvents = 512;

struct midiEvent {
  double time;
  int8_t pitch;
  int8_t velocity;
};

// The state shared between the DSP and the UI.
struct pluguzuState {
  bool isPlaying;
  int32_t lastSyncBar;
  int32_t lastRenderedBar;

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

  /**
     Get the plugin label.@n
     This label is a short restricted name consisting of only _, a-z, A-Z and
     0-9 characters.
   */
  const char *getLabel() const noexcept override { return "Pluguzu"; }

  /**
     Get the plugin author/maker.
   */
  const char *getMaker() const noexcept override { return "tristanC"; }

  /**
     Get the plugin license (a single line of text or a URL).@n
     For commercial plugins this should return some short copyright information.
   */
  const char *getLicense() const noexcept override { return "GPLv3"; }

  /**
     Get the plugin version, in hexadecimal.
     @see d_version()
   */
  uint32_t getVersion() const noexcept override { return d_version(1, 0, 0); }

  // ----------------------------------------------------------------------------------------------------------------
  // Init

  void initState(uint32_t index, State &state) override;

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
  pluguzuState *fState;
  uint32_t currentPos;
  uint32_t currentCount;
  midiEvent currentEvents[maxMidiEvents];
  friend class PluguzuUI;
  void sendMidiEvent(uint32_t pos);

  DISTRHO_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(PluguzuPlugin)
};

END_NAMESPACE_DISTRHO

#endif
