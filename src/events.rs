// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the events produced by Haskell.

use serde::Deserialize;
use smol_str::SmolStr;
use std::collections::HashMap;

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
