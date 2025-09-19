// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Clone, Copy, Deserialize)]
pub struct LocEvent {
    pub col: usize,
    pub row: usize,
    pub len: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[repr(C)]
pub struct ParamEvent {
    pub pos: usize,
    pub param_id: u8,
    pub param_value: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[repr(C)]
pub struct MidiEvent {
    pub start: f64,
    pub stop: f64,
    pub sound: i32,
    pub d0: u8,
    pub d1: u8,
    pub d2: u8,
}

impl MidiEvent {
    pub fn data(&self) -> [u8; 3] {
        [self.d0, self.d1, self.d2]
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub midi: MidiEvent,
    pub locs: Vec<LocEvent>,
    pub sound: Option<crate::samples::SampleName>,
    pub value: Option<HashMap<smol_str::SmolStr, serde_json::Value>>,
}
