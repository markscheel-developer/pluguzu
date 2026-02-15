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
                EventValue::Num(v) => Some(MidiTrigger(*v as usize)),
                _ => None,
            })
    }
}

#[derive(Default, Clone)]
struct MidiTrigger(usize);

// The events are storer per midi trigger.
// 0 is for global events.
// 1..13 is for triggered events (1 is midi C)
#[derive(Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Debug)]
pub struct TriggerIDX(usize);

impl TriggerIDX {
    fn from_mt(mt: MidiTrigger) -> Self {
        Self(mt.0 + 1)
    }
}

#[derive(Default, Clone)]
pub struct Events {
    pub per_trigger: Vec<(TriggerIDX, Vec<Event>)>,
    pub count: usize,
}

impl Events {
    pub fn from_events(events: Vec<Event>) -> Self {
        // Store events per trigger
        let mut per_trigger = HashMap::new();
        let count = events.len();
        for event in events {
            let trigger = match event.midi_trigger() {
                Some(mt) => TriggerIDX::from_mt(mt),
                // no mt, trigger is 0
                _ => TriggerIDX(0),
            };
            per_trigger
                .entry(trigger)
                .or_insert_with(Vec::new)
                .push(event)
        }
        let mut per_trigger: Vec<(TriggerIDX, Vec<Event>)> = per_trigger.drain().collect();
        // Store the events to ensure global events are at the begining
        per_trigger.sort_by(|a, b| a.0.cmp(&b.0));
        Self { per_trigger, count }
    }

    fn pos(&self, trigger: Option<MidiTrigger>) -> Option<TriggerIDX> {
        let val = TriggerIDX(trigger.map(|x| x.0 + 1).unwrap_or(0));
        self.per_trigger
            .iter()
            .position(|x| x.0 == val)
            .map(TriggerIDX)
    }
    fn get_trigger(&self, trigger: Option<MidiTrigger>) -> Option<&[Event]> {
        match self.pos(trigger) {
            None => None,
            Some(idx) => Some(&(self.per_trigger[idx.0].1)),
        }
    }
    fn get(&self, idx: TriggerIDX) -> &[Event] {
        &self.per_trigger[idx.0].1
    }
}

const NO_EVENTS: &[Event] = &[];

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
}

#[derive(Clone, Debug)]
struct ActiveTrigger {
    pos_idx: usize,
    events_idx: TriggerIDX,
    start: f64,
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
            last_start: [0.; TRIGGER_COUNT],
            cycle_length: 4.,
            midi_trigger,
            actives: Vec::with_capacity(TRIGGER_COUNT),
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
    pub fn update_actives(&mut self) {
        self.actives.clear();
        for (idx, start) in self.midi_trigger.starts.iter().enumerate() {
            let start = start.load(Ordering::Relaxed);
            if !start.is_nan() {
                match self.events.pos(Some(MidiTrigger(idx))) {
                    Some(pos) => self.actives.push(ActiveTrigger {
                        pos_idx: idx + 1,
                        events_idx: pos,
                        start,
                    }),
                    None => pwarn!("Unknown trigger: {idx}"),
                }
            }
        }
    }
    pub fn iter(&mut self, range: CycleRange) -> EventsIter<'_> {
        self.update_actives();
        let actives = &self.actives;
        let current_events = self.events.get_trigger(None).unwrap_or(NO_EVENTS);
        let start = range.arc.start % self.cycle_length;

        if range.jumped {
            self.pos[0] = current_events
                .iter()
                .position(|ev| ev.span.start >= start)
                .unwrap_or(usize::MAX);
            for active in actives {
                let tstart = (range.arc.start - active.start) % self.cycle_length;
                self.pos[active.pos_idx] = self.events.per_trigger[active.events_idx.0]
                    .1
                    .iter()
                    .position(|ev| ev.span.start >= tstart)
                    .unwrap_or(usize::MAX);
            }
        }
        for active in actives {
            if self.last_start[active.pos_idx - 1] != active.start {
                pwarn!("Trigger reset! {active:?} {start}");
                self.pos[active.pos_idx] = 0;
            }
            self.last_start[active.pos_idx - 1] = active.start;
        }

        let current_last_pos = self.pos[0];
        EventsIter {
            events: &self.events,
            pos: &mut self.pos,
            current_last_pos,
            current_pos: 0,
            current_arc: CycleArc {
                start,
                end: range.arc.end % self.cycle_length,
            },
            current_events,
            actives_pos: 0,
            actives,
            range,
            cycle_length: self.cycle_length,
        }
    }
}

pub struct EventsIter<'a> {
    events: &'a Events,
    pos: &'a mut [usize],
    actives: &'a [ActiveTrigger],
    actives_pos: usize,
    current_pos: usize,
    current_events: &'a [Event],
    current_arc: CycleArc,
    current_last_pos: usize,
    range: CycleRange,
    cycle_length: f64,
}

impl EventsIter<'_> {
    pub fn next(&mut self) -> Option<(&Event, f64)> {
        // if self.current_pos > 0 {
        //     crate::debug!(
        //         "pos: {:?}, actives: {:?}, actives_pos: {}, current_pos: {}, current_last_pos: {}",
        //         self.pos,
        //         self.actives,
        //         self.actives_pos,
        //         self.current_pos,
        //         self.current_last_pos
        //     );
        // }
        let pos = &mut self.pos[self.current_pos];
        match next_event(
            &self.current_arc,
            pos,
            self.current_last_pos,
            self.current_events,
        ) {
            Some(ev) => Some(ev),
            None => match self.actives.get(self.actives_pos) {
                None => None,
                Some(active) => {
                    self.actives_pos += 1;
                    self.current_arc = CycleArc {
                        start: (self.range.arc.start - active.start) % self.cycle_length,
                        end: (self.range.arc.end - active.start) % self.cycle_length,
                    };
                    self.current_events = self.events.get(active.events_idx);
                    self.current_last_pos = self.pos[active.pos_idx];
                    self.current_pos = active.pos_idx;
                    self.next()
                }
            },
        }
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
