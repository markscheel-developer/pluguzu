// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the events produced by Haskell.

use serde::Deserialize;
use smol_str::SmolStr;
use std::{collections::HashMap, sync::Arc};

use crate::transport::CycleRange;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy, Deserialize)]
pub struct LocEvent {
    pub col: usize,
    pub row: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[repr(C)]
pub struct MidiEvent {
    pub d0: u8,
    pub d1: u8,
    pub d2: u8,
}

impl MidiEvent {
    pub fn data(&self) -> [u8; 3] {
        [self.d0, self.d1, self.d2]
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct EventSpan {
    pub start: f64,
    pub stop: f64,
}

#[derive(Debug, Clone, Deserialize, Ord, Eq, PartialOrd, PartialEq, Hash)]
pub struct SoundEvent {
    pub name: SmolStr,
    pub idx: i32,
    pub note: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EventKind {
    EventMidi(MidiEvent),
    EventSound(SoundEvent),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EventValue {
    String(SmolStr),
    Num(f32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub span: EventSpan,
    pub kind: EventKind,
    pub locs: Vec<LocEvent>,
    pub value: Option<HashMap<SmolStr, EventValue>>,
}

const NO_BANK: SmolStr = SmolStr::new_static("");
const BANK: SmolStr = SmolStr::new_static("bank");

impl Event {
    pub fn has_legato(&self) -> bool {
        self.value
            .as_ref()
            .map(|vs| vs.contains_key("legato"))
            .unwrap_or(false)
    }
    pub fn bank(&self) -> SmolStr {
        if let Some(EventValue::String(bank)) =
            self.value.as_ref().and_then(|value| value.get(&BANK))
        {
            bank.clone()
        } else {
            NO_BANK
        }
    }
}

#[derive(Default, Clone)]
pub struct EventsBuffer {
    pub events: Arc<Vec<Event>>,
    pub pos: usize,
    pub cycle_length: f64,
    pub midi_trigger: Option<f64>,
}

impl EventsBuffer {
    // Reset the buffer after a new update from the background thread
    pub(crate) fn reset(&mut self, events: Arc<Vec<Event>>, cycle_length: f64) {
        self.events = events;
        self.cycle_length = cycle_length;
        self.pos = 0;
    }
    pub fn take(&self) -> Arc<Vec<Event>> {
        self.events.clone()
    }
    pub fn remaining(&self) -> usize {
        self.events.len().saturating_sub(self.pos)
    }
    pub fn iter(&mut self, range: CycleRange) -> EventsIter<'_> {
        let offset = self.midi_trigger.unwrap_or(0.);
        let start = (range.arc.start - offset) % self.cycle_length;
        let end = (range.arc.end - offset) % self.cycle_length;
        if range.jumped {
            self.pos = self
                .events
                .iter()
                .position(|ev| ev.span.start >= start)
                .unwrap_or(usize::MAX);
        }
        let last_pos = self.pos;
        EventsIter {
            buffer: self,
            last_pos,
            start,
            end,
        }
    }
}

pub struct EventsIter<'a> {
    buffer: &'a mut EventsBuffer,
    last_pos: usize,
    start: f64,
    end: f64,
}

impl EventsIter<'_> {
    pub fn next(&mut self) -> Option<(&Event, f64)> {
        let start = self.start;
        let end = self.end;
        if self.buffer.pos == usize::MAX {
            // waiting for the cycles to restart...
            if start == 0. || start > end {
                self.buffer.pos = 0
            } else {
                return None;
            }
        }
        if self.buffer.events.is_empty() {
            return None;
        }
        let ev = &self.buffer.events[self.buffer.pos];

        let span_start = if start < end {
            // Non overlapping range
            if ev.span.start >= end {
                // This event starts in the future.
                return None;
            } else if ev.span.start >= start {
                // This event is due for this buffer.
                start + (ev.span.start - self.start)
            } else {
                // #[rustfmt::skip]
                // crate::pwarn!("{} Missed an event? {:.7}..{:.7} for ev.start={:.7}", pos, start, end, ev.span.start);
                return None;
            }
        } else {
            // Overlapping range
            if ev.span.start >= start || ev.span.start < end {
                end - ev.span.start
            } else {
                return None;
            }
        };

        self.buffer.pos += 1;
        if self.last_pos == self.buffer.pos {
            // looped around
            return None;
        }
        if self.buffer.pos >= self.buffer.events.len() {
            self.buffer.pos = usize::MAX;
        }

        Some((ev, span_start))
    }
}
