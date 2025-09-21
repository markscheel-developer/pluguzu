// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines midi (and sound) event management.

use crate::{
    debug,
    events::{Event, EventKind, EventSpan, MidiEvent, SoundEvent},
    pwarn, Pluguzu,
};
use nih_plug::{midi::NoteEvent, nih_error, prelude::ProcessContext};

#[derive(Debug)]
pub struct FutureNoteOff {
    when: f64,
    channel: u8,
}

/// Information to get midi event offset.
pub struct ProcessMidi {
    /// The process buffer start fraction.
    pub start: f64,
    /// The process buffer stop fraction.
    pub end: f64,
    /// The number of samples per whole fraction.
    pub frames_per_fraction: f64,
}

impl ProcessMidi {
    /// Calculate the event timing offset withing the current process buffer.
    fn event_offset(&self, when: f64) -> u32 {
        ((when - self.start) * self.frames_per_fraction) as u32
    }

    /// Send a note-off event with a pre-computed timing.
    fn do_send_note_off(
        &self,
        timing: u32,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        let mev = NoteEvent::NoteOff {
            timing,
            voice_id: None,
            channel: ev.channel,
            note,
            velocity: 0.,
        };
        debug!("{:.4}: sending note off      {:?}", self.start, mev);
        context.send_event(mev);
    }

    /// Send a note-off event.
    fn send_note_off(
        &self,
        note: u8,
        ev: &FutureNoteOff,
        context: &mut impl ProcessContext<Pluguzu>,
    ) {
        self.do_send_note_off(self.event_offset(ev.when), note, ev, context)
    }
}

impl Pluguzu {
    pub fn send_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        let midi = &self.midi;
        self.note_offs.retain(|note, ev| {
            if ev.when >= midi.start && ev.when < midi.end {
                midi.send_note_off(*note, ev, context);
                false
            } else {
                true
            }
        });
    }

    pub fn send_all_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        self.note_offs.retain(|note, ev| {
            self.midi.do_send_note_off(0, *note, ev, context);
            false
        });
    }

    pub fn send_midi_event(
        &mut self,
        span: &EventSpan,
        ev: &MidiEvent,
        context: &mut impl ProcessContext<Self>,
    ) {
        let offset = if span.start > self.midi.start {
            self.midi.event_offset(span.start)
        } else {
            0
        };
        match NoteEvent::<()>::from_midi(offset, &ev.data()) {
            Ok(mev) => {
                debug!(
                    "{:.4}: sending event {:.4} {:?}",
                    self.midi.start, span.start, mev
                );
                context.send_event(mev);
                if let NoteEvent::NoteOn { note, channel, .. } = mev {
                    if span.stop < self.midi.end {
                        // This is a fast event, schedule the note-off right away
                        self.midi.send_note_off(
                            note,
                            &FutureNoteOff {
                                when: span.stop,
                                channel,
                            },
                            context,
                        )
                    } else {
                        // Schedule the note-off for later
                        let prev = self.note_offs.insert(
                            note,
                            FutureNoteOff {
                                when: span.stop,
                                channel,
                            },
                        );
                        if let Some(prev) = prev {
                            // Send any existing note-off
                            self.midi.do_send_note_off(0, note, &prev, context)
                        }
                    }
                }
            }
            Err(err) => nih_error!("Couldn't decode {:?} {:?}", ev, err),
        }
    }

    fn send_sound_event(&mut self, ev: &Event, sev: &SoundEvent) {
        let samples_library = self.samples_library.read().expect("Samples read lock");
        match samples_library.get(sev) {
            Some(samples) => {
                let mut sound = crate::dirt::Sound::new(48000).expect("Valid new_sound");
                if let Some(value) = ev.value.as_ref() {
                    for (k, v) in value.iter() {
                        if let Some(val) = v.as_f64() {
                            sound.apply_param(k, val as f32)
                        } else if let Some(val) = v.as_str().and_then(|s| s.chars().nth(0)) {
                            sound.apply_param_char(k, val)
                        }
                    }
                }
                debug!(
                    "{:.4}: playing sample {} with {ev:?}",
                    ev.span.start, sev.name
                );
                let frame_offset = self.midi.event_offset(ev.span.start) as f64;
                let sample_time = 1. / (self.dirt.sr as f64);
                let when = self.now + sample_time * frame_offset;
                sound.play(when, self.cps, samples)
            }
            None => {
                pwarn!("Sample not ready! {:?}", sev);
            }
        };
    }

    fn send_tidal_event(&mut self, ev: &Event, context: &mut impl ProcessContext<Self>) {
        match &ev.kind {
            EventKind::EventSound(sev) => self.send_sound_event(ev, sev),
            EventKind::EventMidi(mev) => self.send_midi_event(&ev.span, mev, context),
        }
    }

    pub fn send_tidal_events(
        &mut self,
        start_fraction: f64,
        stop_fraction: f64,
        events: &[Event],
        context: &mut impl ProcessContext<Self>,
    ) {
        while self.events_pos < events.len() {
            let ev = &events[self.events_pos];
            if ev.span.start >= stop_fraction {
                // This event and the following one are scheduled for later
                break;
            } else if start_fraction <= ev.span.start && ev.span.start < stop_fraction {
                // The event is due for this buffer
                self.send_tidal_event(ev, context);
            } else if ev.span.stop > 0. && ev.span.stop <= stop_fraction {
                pwarn!(
                    "{start_fraction:.4} Missed event start, but it still make sense to send it now... {:.4}..{:.4} < {stop_fraction:.4}",
                    ev.span.start, ev.span.stop
                );
                self.send_tidal_event(ev, context);
            } else if start_fraction > ev.span.start {
                // That's odd, it looks like we missed an event?!
                pwarn!("{start_fraction:.4} Skipped event {:.4}", ev.span.start);
            }

            // Advance the event buffer position
            self.events_pos += 1;
        }
    }
}
