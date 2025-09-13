// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::{binding::MidiEvent, Pluguzu};
use nih_plug::{midi::NoteEvent, prelude::ProcessContext};

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
        println!("{:.4}: sending note off      {:?}", self.start, mev);
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

    pub fn send_event(&mut self, ev: &MidiEvent, context: &mut impl ProcessContext<Self>) {
        let offset = if ev.start > self.midi.start {
            self.midi.event_offset(ev.start)
        } else {
            0
        };
        match NoteEvent::<()>::from_midi(offset, &ev.data) {
            Ok(mev) => {
                println!(
                    "{:.4}: sending event {:.4} {:?}",
                    self.midi.start, ev.start, mev
                );
                context.send_event(mev);
                if let NoteEvent::NoteOn { note, channel, .. } = mev {
                    if ev.stop < self.midi.end {
                        // This is a fast event, schedule the note-off right away
                        self.midi.send_note_off(
                            note,
                            &FutureNoteOff {
                                when: ev.stop,
                                channel,
                            },
                            context,
                        )
                    } else {
                        // Schedule the note-off for later
                        let prev = self.note_offs.insert(
                            note,
                            FutureNoteOff {
                                when: ev.stop,
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
            Err(err) => println!("Couldn't decode {:?} {:?}", ev, err),
        }
    }

    pub fn send_events(
        &mut self,
        start_fraction: f64,
        stop_fraction: f64,
        context: &mut impl ProcessContext<Self>,
    ) {
        while self.buffer_pos < self.buffer.len() {
            let ev = self.buffer[self.buffer_pos];
            if ev.start >= stop_fraction {
                // This event and the following one are scheduled for later
                break;
            } else if start_fraction <= ev.start && ev.start < stop_fraction {
                // The event is due for this buffer
                self.send_event(&ev, context);
            } else if ev.stop > 0. && ev.stop <= stop_fraction {
                println!(
                    "{start_fraction:.4} Missed event start, but it still make sense to send it now... {:.4} - {:.4} < {stop_fraction:.4}",
                    ev.start, ev.stop
                );
                self.send_event(&ev, context);
            } else if start_fraction > ev.start {
                // That's odd, it looks like we missed an event?!
                println!("{start_fraction:.4} Skipped event {:.4}", ev.start);
            }

            // Advance the event buffer position
            self.buffer_pos += 1;
        }
    }
}
