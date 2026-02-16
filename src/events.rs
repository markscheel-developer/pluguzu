// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the events produced by Haskell.

use atomic_float::AtomicF64;
use nih_plug::midi::NoteEvent;
use serde::Deserialize;
use smol_str::SmolStr;
use std::{
    collections::HashMap,
    sync::{Arc, atomic::Ordering},
};

use crate::{
    PluguzuTransport, pwarn,
    transport::{CycleArc, CycleRange},
};

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
    fn midi_trigger(&self) -> Option<MidiTrigger> {
        self.value
            .as_ref()
            .and_then(|v| v.get(&SmolStr::new_static("mt")))
            .and_then(|v| match v {
                EventValue::Num(v) => Some(MidiTrigger(*v as usize % TRIGGER_COUNT)),
                _ => None,
            })
    }
}

#[derive(Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Debug)]
struct MidiTrigger(usize);

#[derive(Default, Clone)]
pub struct Events {
    pub per_trigger: [Vec<Event>; TRIGGER_COUNT + 1],
    pub count: usize,
}

impl Events {
    pub fn from_events(events: Vec<Event>) -> Self {
        let mut per_trigger: [Vec<Event>; TRIGGER_COUNT + 1] = Default::default();
        let count = events.len();
        for event in events {
            match event.midi_trigger() {
                Some(mt) => per_trigger[mt.0 + 1].push(event),
                // no mt, trigger is 0
                _ => per_trigger[0].push(event),
            };
        }
        Self { per_trigger, count }
    }
}

const TRIGGER_COUNT: usize = 12;
pub struct MidiTriggers {
    starts: Vec<AtomicF64>,
}

impl Default for MidiTriggers {
    fn default() -> Self {
        let mut starts = Vec::with_capacity(12);
        for _i in 0..TRIGGER_COUNT {
            starts.push(AtomicF64::new(f64::NAN))
        }
        Self { starts }
    }
}
impl MidiTriggers {
    pub fn trigger(&self, note: &u8, start: Option<f64>) {
        let note = *note as usize % TRIGGER_COUNT;
        let value = start.unwrap_or(f64::NAN);
        pwarn!("MIDI Triggers! {note} {start:?}");
        self.starts[note].store(value, Ordering::Relaxed);
    }
    pub(crate) fn update_from_event(&self, transport: &PluguzuTransport, event: &NoteEvent<()>) {
        match event {
            NoteEvent::NoteOn { note, .. } => {
                // TODO: handle timing offset
                let start = transport.range.arc.start;
                self.trigger(note, Some(start));
                // auto play on first note
                if !transport.state.synced.load(Ordering::Relaxed) {
                    transport.state.playing.store(true, Ordering::Relaxed);
                }
            }
            NoteEvent::NoteOff { note, .. } => self.trigger(note, None),
            _ => {}
        };
    }
    fn iter(&self) -> impl Iterator<Item = (MidiTrigger, f64)> {
        self.starts
            .iter()
            .enumerate()
            .map(|(x, y)| (MidiTrigger(x), y.load(Ordering::Relaxed)))
    }
}

#[derive(Clone, Debug)]
struct ActiveTrigger {
    idx: usize,
    last_pos: usize,
    arc: CycleArc,
}

#[derive(Clone)]
pub struct EventsBuffer {
    pub events: Arc<Events>,
    pub pos: [usize; TRIGGER_COUNT + 1],
    pub last_start: [f64; TRIGGER_COUNT],
    pub cycle_length: f64,
    actives: Vec<ActiveTrigger>,
    pub midi_trigger: Arc<MidiTriggers>,
}

impl Default for EventsBuffer {
    fn default() -> Self {
        Self::with_trigger(Arc::new(MidiTriggers::default()))
    }
}

impl EventsBuffer {
    pub fn with_trigger(midi_trigger: Arc<MidiTriggers>) -> Self {
        Self {
            events: Arc::new(Events::default()),
            pos: [0; TRIGGER_COUNT + 1],
            last_start: [f64::NAN; TRIGGER_COUNT],
            cycle_length: 4.,
            midi_trigger,
            actives: Vec::with_capacity(TRIGGER_COUNT + 1),
        }
    }
    // Reset the buffer after a new update from the background thread
    pub fn reset(&mut self, events: Arc<Events>, cycle_length: f64) {
        self.events = events;
        self.cycle_length = cycle_length;
        self.pos.fill(0);
    }
    pub fn take(&self) -> Arc<Events> {
        self.events.clone()
    }
    pub fn remaining(&self) -> usize {
        0
        // self.events.len().saturating_sub(self.pos)
        //TODO: compute based on active triggers
    }
    pub fn update_actives(&mut self, arc: CycleArc) {
        self.actives.clear();
        if !self.events.per_trigger[0].is_empty() {
            self.actives.push(ActiveTrigger {
                idx: 0,
                last_pos: self.pos[0],
                arc: CycleArc {
                    start: arc.start % self.cycle_length,
                    end: arc.end % self.cycle_length,
                },
            })
        }
        for (mt, start) in self.midi_trigger.iter() {
            if !start.is_nan() {
                let idx = mt.0 + 1;
                if !self.events.per_trigger[idx].is_empty() {
                    if self.last_start[mt.0] != start {
                        pwarn!("Trigger reset! {mt:?} {start}");
                        self.pos[idx] = 0;
                    }
                    self.last_start[mt.0] = start;
                    self.actives.push(ActiveTrigger {
                        idx,
                        last_pos: self.pos[idx],
                        arc: CycleArc {
                            start: (arc.start - start) % self.cycle_length,
                            end: (arc.end - start) % self.cycle_length,
                        },
                    })
                } else {
                    pwarn!("Unknown trigger: {idx}")
                }
            }
        }
    }
    pub fn iter(&mut self, range: CycleRange) -> EventsIter<'_> {
        self.update_actives(range.arc);

        if range.jumped {
            let start = range.arc.start % self.cycle_length;
            self.pos[0] = self.events.per_trigger[0]
                .iter()
                .position(|ev| ev.span.start >= start)
                .unwrap_or(usize::MAX);
        }
        let queue = smallvec::SmallVec::<[(&Event, f64); TRIGGER_COUNT + 1]>::new();
        EventsIter {
            events: &self.events,
            pos: &mut self.pos,
            actives: &mut self.actives,
            queue: queue,
        }
    }
}

pub struct EventsIter<'a> {
    events: &'a Events,
    pos: &'a mut [usize],
    actives: &'a mut [ActiveTrigger],
    queue: smallvec::SmallVec<[(&'a Event, f64); TRIGGER_COUNT + 1]>,
}

impl EventsIter<'_> {
    pub fn fast_next(&mut self) -> Option<&Event> {
        // for the ui, don't sort
        for active in self.actives.iter() {
            match next_event(
                &active.arc,
                &mut self.pos[active.idx],
                active.last_pos,
                &self.events.per_trigger[active.idx],
            ) {
                Some((ev, _)) => return Some(ev),
                None => {}
            }
        }
        None
    }
    pub fn next(&mut self) -> Option<(&Event, f64)> {
        for active in self.actives.iter() {
            next_event(
                &active.arc,
                &mut self.pos[active.idx],
                active.last_pos,
                &self.events.per_trigger[active.idx],
            )
            .map(|ev| {
                self.queue.push(ev);
            });
        }
        self.queue.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.queue.pop()
    }
}

fn next_event<'a>(
    arc: &CycleArc,
    pos: &mut usize,
    last_pos: usize,
    events: &'a [Event],
) -> Option<(&'a Event, f64)> {
    let start = arc.start;
    let end = arc.end;
    if *pos == usize::MAX {
        // waiting for the cycles to restart...
        if start >= end {
            *pos = 0
        } else {
            return None;
        }
    }
    if events.is_empty() {
        return None;
    }
    let ev = &events[*pos];

    let span_start = if start < end {
        // Non overlapping range
        if ev.span.start >= end {
            // This event starts in the future.
            return None;
        } else if ev.span.start >= start {
            // This event is due for this buffer.
            start + (ev.span.start - start)
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

    *pos += 1;
    if last_pos == *pos {
        // looped around
        return None;
    }
    if *pos >= events.len() {
        *pos = usize::MAX;
    }

    Some((ev, span_start))
}
