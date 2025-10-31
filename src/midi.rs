// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines midi (and sound) event management.

use crate::{
    Pluguzu, debug,
    events::{Event, EventKind, EventSpan, EventValue, MidiEvent, SoundEvent},
    pwarn,
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

pub struct ProcessAudio {
    /// The cycle per second, for dough.
    pub cycle_length: f64,
    /// The current time position in second.
    pub now: f64,
    /// The sample rate.
    pub sample_rate: u32,
}

impl Pluguzu {
    pub fn send_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        let midi = &self.midi;
        self.note_offs.retain(|(note, _), ev| {
            if ev.when >= midi.start && ev.when < midi.end {
                midi.send_note_off(*note, ev, context);
                false
            } else {
                true
            }
        });
    }

    pub fn send_all_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        self.note_offs.retain(|(note, _), ev| {
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
                            (note, channel),
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

    fn send_sound_event(&mut self, ev: &Event, sev: &SoundEvent, buffer: &mut [&mut [f32]]) {
        let mut sound = crate::dirt::Sound::new();
        if sound.default_sound(&sev.name.as_str()) {
            sound.apply_param(
                "duration",
                ((ev.span.stop - ev.span.start) * self.audio.cycle_length) as f32,
            );
        } else {
            match self.samples_library.get(ev.bank(), sev) {
                Some(samples) => {
                    sound.file_source(&samples);
                }
                None => {
                    pwarn!("Sample not ready! {:?}", sev);
                    return;
                }
            }
        }
        if let Some(value) = ev.value.as_ref() {
            for (k, v) in value.iter() {
                if let EventValue::Num(val) = v {
                    if k == "legato" {
                        sound.0.duration *= *val
                    } else {
                        sound.apply_param(k, *val)
                    }
                }
            }
        }
        debug!(
            "{:.4}: playing sample {} at {}Hz for {} with {ev:?}",
            ev.span.start, sev.name, sound.0.freq, sound.0.duration,
        );
        let frame_offset = self.midi.event_offset(ev.span.start) as usize;
        self.dirt.advance(frame_offset.saturating_sub(1), buffer);
        self.dirt.process(&sound);
    }

    fn send_tidal_event(
        &mut self,
        ev: &Event,
        buffer: &mut [&mut [f32]],
        context: &mut impl ProcessContext<Self>,
    ) {
        match &ev.kind {
            EventKind::EventSound(sev) => self.send_sound_event(ev, sev, buffer),
            EventKind::EventMidi(mev) => self.send_midi_event(&ev.span, mev, context),
        }
    }

    pub fn send_tidal_events(
        &mut self,
        events: &[Event],
        buffer: &mut [&mut [f32]],
        context: &mut impl ProcessContext<Self>,
    ) {
        while self.events_pos < events.len() {
            let ev = &events[self.events_pos];
            if ev.span.start >= self.midi.end {
                // This event (and the following ones) are scheduled for later
                break;
            } else if self.midi.start <= ev.span.start {
                // The event is due for this buffer
                self.send_tidal_event(ev, buffer, context);
            } else if ev.span.stop >= self.midi.end {
                pwarn!(
                    "{:.7} Missed event start, but it still make sense to send it now... {:.7}..{:.7} < {:.7}",
                    self.midi.start,
                    ev.span.start,
                    ev.span.stop,
                    self.midi.end
                );
                self.send_tidal_event(ev, buffer, context);
            } else if self.midi.start > ev.span.start {
                // That's odd, it looks like we missed an event?!
                pwarn!("{:.4} Skipped event {:.4}", self.midi.start, ev.span.start);
            }

            // Advance the event buffer position
            self.events_pos += 1;
        }
    }
}
