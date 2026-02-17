// Copyright © 2026 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::sync::Arc;

use crate::{
    events::{Event, MidiTriggers, TRIGGER_COUNT},
    pwarn,
    transport::{CycleArc, CycleRange},
};

#[derive(Default, Clone)]
pub struct Events {
    pub per_trigger: [Vec<Event>; TRIGGER_COUNT + 1],
    pub count: usize,
    pub bar_start: i32,
    pub bar_count: i32,
}

impl Events {
    pub fn from_events(events: Vec<Event>, start: i32, end: i32) -> Self {
        let mut per_trigger: [Vec<Event>; TRIGGER_COUNT + 1] = Default::default();
        let count = events.len();
        for event in events {
            match event.midi_trigger() {
                Some(mt) => per_trigger[mt.0 + 1].push(event),
                // no mt, trigger is 0
                _ => per_trigger[0].push(event),
            };
        }
        Self {
            per_trigger,
            count,
            bar_start: start,
            bar_count: end - start,
        }
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
    pub need_reset: [bool; TRIGGER_COUNT],
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
            need_reset: [false; TRIGGER_COUNT],
            midi_trigger,
            actives: Vec::with_capacity(TRIGGER_COUNT + 1),
        }
    }
    // Reset the buffer after a new update from the background thread
    pub fn reset(&mut self, events: Arc<Events>) {
        self.events = events;
        self.pos.fill(0);
    }
    pub fn take(&self) -> Arc<Events> {
        self.events.clone()
    }
    pub fn remaining(&self) -> usize {
        self.events.per_trigger[0].len().saturating_sub(self.pos[0])
    }
    pub fn update_actives(&mut self, arc: CycleArc, synced: bool) {
        self.actives.clear();
        if !self.events.per_trigger[0].is_empty() {
            self.actives.push(ActiveTrigger {
                idx: 0,
                last_pos: self.pos[0],
                arc: if synced {
                    // When synced, the arc wrap around the static buffer
                    arc.modulo(self.events.bar_count, 0.)
                } else {
                    // Otherwise the arc is continuously moving forward
                    arc
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
                        arc: if synced {
                            // When synce#, the arc wrap around the static buffer
                            arc.modulo(self.events.bar_count, start)
                        } else {
                            // Here is the tricky part... The events buffer is continuous,
                            // but the triggered pattern should always start at the first event
                            // within the region. This is using the fixed buffer length to determine
                            // where in the buffer the trigger is presently playing.
                            let base = self.events.bar_start as f64;
                            let count = (self.events.bar_count - 1) as f64;
                            let start = base + (arc.start - start) % count;
                            if self.need_reset[mt.0] {
                                // When the trigger starts mid way through a buffer, we might have to
                                // catchup to the current event position after it reach the end of the buffer...
                                pwarn!("{arc} Reseting trigger {} at {start}", mt.0);
                                self.pos[idx] = self.events.per_trigger[idx]
                                    .iter()
                                    .position(|ev| ev.span.start >= start)
                                    .unwrap_or(usize::MAX);
                                self.need_reset[mt.0] = false;
                            }
                            let end = start + (arc.end - arc.start);
                            if end > base + count {
                                self.need_reset[mt.0] = true;
                            }
                            CycleArc { start, end }
                        },
                    });
                } else {
                    // pwarn!("Unknown trigger: {idx}")
                }
            }
        }
    }
    pub fn iter(&mut self, range: CycleRange, synced: bool) -> EventsIter<'_> {
        self.update_actives(range.arc, synced);

        if range.jumped {
            for active in self.actives.iter_mut() {
                self.pos[active.idx] = self.events.per_trigger[active.idx]
                    .iter()
                    .position(|ev| ev.span.start >= active.arc.start)
                    .unwrap_or(usize::MAX);
                active.last_pos = self.pos[active.idx];
            }
        }
        let queue = smallvec::SmallVec::<[(&Event, f64); TRIGGER_COUNT + 1]>::new();
        // pwarn!("HERE: {} {} {:?}", range.arc, self.pos[0], self.actives);
        EventsIter {
            events: &self.events,
            pos: &mut self.pos,
            actives: &mut self.actives,
            queue,
            continuous: !synced,
        }
    }
}

pub struct EventsIter<'a> {
    events: &'a Events,
    pos: &'a mut [usize],
    actives: &'a mut [ActiveTrigger],
    queue: smallvec::SmallVec<[(&'a Event, f64); TRIGGER_COUNT + 1]>,
    continuous: bool,
}

impl EventsIter<'_> {
    pub fn fast_next(&mut self) -> Option<&Event> {
        // for the ui, don't sort
        for active in self.actives.iter() {
            if let Some((ev, _)) = next_event(
                self.continuous,
                &active.arc,
                &mut self.pos[active.idx],
                active.last_pos,
                &self.events.per_trigger[active.idx],
            ) {
                return Some(ev);
            }
        }
        None
    }
    pub fn next_ev(&mut self) -> Option<(&Event, f64)> {
        for active in self.actives.iter() {
            if let Some(ev) = next_event(
                self.continuous,
                &active.arc,
                &mut self.pos[active.idx],
                active.last_pos,
                &self.events.per_trigger[active.idx],
            ) {
                self.queue.push(ev);
            }
        }
        self.queue.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.queue.pop()
    }
}

fn next_continuous_event<'a>(
    arc: &CycleArc,
    pos: &mut usize,
    events: &'a [Event],
) -> Option<(&'a Event, f64)> {
    events.get(*pos).and_then(|event| {
        if event.span.start >= arc.start && event.span.start < arc.end {
            *pos += 1;
            Some((event, event.span.start))
        } else {
            None
        }
    })
}

fn next_event<'a>(
    continuous: bool,
    arc: &CycleArc,
    pos: &mut usize,
    last_pos: usize,
    events: &'a [Event],
) -> Option<(&'a Event, f64)> {
    if continuous {
        return next_continuous_event(arc, pos, events);
    }
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
