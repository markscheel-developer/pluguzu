// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines midi (and sound) event management.

use crate::{
    Pluguzu, PluguzuTransport, debug,
    events::{Event, EventKind, EventSpan, EventValue, MidiEvent, SoundEvent},
    pwarn,
};
use nih_plug::{midi::NoteEvent, nih_error, prelude::ProcessContext};

#[derive(Debug)]
pub struct FutureNoteOff {
    when: f64,
    channel: u8,
}

impl PluguzuTransport {
    /// Calculate the event timing offset withing the current process buffer.
    fn event_offset(&self, when: f64) -> u32 {
        ((when - self.range.arc.start) * self.cycle_transport.frames_per_cycle) as u32
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
        debug!("{}: sending note off - {:?}", self.range.arc, mev);
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
        let transport = &self.transport;
        self.note_offs.retain(|(note, _), ev| {
            if ev.when >= transport.range.arc.start && ev.when < transport.range.arc.end {
                transport.send_note_off(*note, ev, context);
                false
            } else {
                true
            }
        });
    }

    pub fn send_all_note_offs(&mut self, context: &mut impl ProcessContext<Self>) {
        self.note_offs.retain(|(note, _), ev| {
            self.transport.do_send_note_off(0, *note, ev, context);
            false
        });
    }

    pub fn send_midi_event(
        &mut self,
        span: &EventSpan,
        span_start: f64,
        ev: &MidiEvent,
        context: &mut impl ProcessContext<Self>,
    ) {
        let offset = if span_start > self.transport.range.arc.start {
            self.transport.event_offset(span_start)
        } else {
            0
        };
        match NoteEvent::<()>::from_midi(offset, &ev.data()) {
            Ok(mev) => {
                let off_when = span_start + (span.stop - span.start);
                if let NoteEvent::NoteOn { note, channel, .. } = mev {
                    if off_when < self.transport.range.arc.end {
                        // This is a fast event, schedule the note-off right away
                        self.transport.send_note_off(
                            note,
                            &FutureNoteOff {
                                when: off_when,
                                channel,
                            },
                            context,
                        )
                    } else {
                        // Schedule the note-off for later
                        let prev = self.note_offs.insert(
                            (note, channel),
                            FutureNoteOff {
                                when: off_when,
                                channel,
                            },
                        );
                        if let Some(prev) = prev {
                            // Send any existing note-off
                            self.transport.do_send_note_off(0, note, &prev, context)
                        }
                    }
                }
                debug!(
                    "{}: sending event {:.4} {:.4} {} {:?}",
                    self.transport.range.arc, span.start, span_start, offset, mev
                );
                nih_plug::util::permit_alloc(|| context.send_event(mev));
            }
            Err(err) => nih_error!("Couldn't decode {:?} {:?}", ev, err),
        }
    }

    fn send_sound_event(
        &mut self,
        ev: &Event,
        span_start: f64,
        sev: &SoundEvent,
        buffer: &mut [&mut [f32]],
    ) {
        let mut sound = crate::dough::Sound::new();
        sound.apply_param(
            "duration",
            ((ev.span.stop - ev.span.start) * self.transport.cycle_transport.cycle_length_secs)
                as f32,
        );
        if !sound.default_sound(sev.name.as_str()) {
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
        let frame_offset = self.transport.event_offset(span_start) as usize;
        self.dough.advance(frame_offset.saturating_sub(1), buffer);
        self.dough.process(&sound);
    }

    fn send_tidal_event(
        &mut self,
        ev: &Event,
        span_start: f64,
        buffer: &mut [&mut [f32]],
        context: &mut impl ProcessContext<Self>,
    ) {
        match &ev.kind {
            EventKind::EventSound(sev) => self.send_sound_event(ev, span_start, sev, buffer),
            EventKind::EventMidi(mev) => self.send_midi_event(&ev.span, span_start, mev, context),
        }
    }

    pub fn send_tidal_events(
        &mut self,
        events: &[Event],
        buffer: &mut [&mut [f32]],
        context: &mut impl ProcessContext<Self>,
    ) {
        let length = self.get_cycle_count();
        let start = self.transport.range.arc.start % length;
        let end = self.transport.range.arc.end % length;

        #[rustfmt::skip]
        // debug!("{start:.7}..{end:.7} Sending events ({}/{}) (for {})", self.events_pos, events.len(), self.transport.range.arc);
        let last_pos = self.events_pos;
        while let Some(ev) =
            crate::events::next_event(events, last_pos, &mut self.events_pos, start, end)
        {
            let span_start = if ev.span.start >= start {
                self.transport.range.arc.start + (ev.span.start - start)
            } else {
                self.transport.range.arc.end - ev.span.start
            };
            self.send_tidal_event(ev, span_start, buffer, context);
        }
    }
}
