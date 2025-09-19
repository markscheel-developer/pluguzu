// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::collections::BTreeMap;
use std::ffi::{c_char, c_double, c_long, c_ulong};

#[link(name = "pluguzu")]
unsafe extern "C" {
    fn pluguzu_init();
    fn pluguzuNew(max_event: c_ulong, max_loc: c_ulong, max_param: c_ulong) -> *mut CPluguzuState;
    fn pluguzuParse(state: *mut CPluguzuState, code: *const c_char);
    fn pluguzuError(state: *mut CPluguzuState) -> *const c_char;
    fn pluguzuRender(
        state: *mut CPluguzuState,
        start: c_ulong,
        end: c_ulong,
        buf: *mut MidiEvent,
        locCount: &mut c_ulong,
        loc: *mut LocEvent,
        paramCount: &mut c_ulong,
        param: *mut ParamEvent,
    ) -> c_ulong;
    fn pluguzuSoundName(state: *mut CPluguzuState, value: i32) -> *const c_char;
}

pub struct PluguzuState {
    max_event: usize,
    max_loc: usize,
    max_param: usize,
    state: *mut CPluguzuState,
    pub buffer: Vec<MidiEvent>,
    pub locs: Vec<LocEvent>,
    pub params: Vec<ParamEvent>,
    pub bar_start: Option<i32>,
    pub sounds: BTreeMap<i32, &'static str>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MidiEvent {
    pub start: c_double,
    pub stop: c_double,
    pub sound: i32,
    pub data: [u8; 3],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LocEvent {
    pub pos: c_long,
    pub col: c_long,
    pub row: c_long,
    pub len: c_long,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ParamEvent {
    pub pos: c_long,
    pub param_id: u8,
    pub param_value: f32,
}

impl PluguzuState {
    pub fn new(max_event: usize, max_loc: usize, max_param: usize) -> Self {
        let state = unsafe {
            pluguzu_init();
            pluguzuNew(max_event as u64, max_loc as u64, max_param as u64)
        };
        PluguzuState {
            max_event,
            max_loc,
            max_param,
            state,
            buffer: Vec::with_capacity(max_event),
            locs: Vec::with_capacity(max_loc),
            params: Vec::with_capacity(max_param),
            bar_start: None,
            sounds: BTreeMap::new(),
        }
    }
    pub fn parse(&mut self, code: &str) -> Option<String> {
        unsafe {
            pluguzuParse(
                self.state,
                std::ffi::CString::new(code).unwrap().as_ptr() as *const c_char,
            );
            let c_str = pluguzuError(self.state);
            if c_str.is_null() {
                None
            } else {
                Some(
                    std::ffi::CStr::from_ptr(c_str)
                        .to_string_lossy()
                        .into_owned(),
                )
            }
        }
    }
    pub fn render(&mut self, start: i32, end: i32) {
        unsafe {
            let mut loc_count_u64: u64 = 0;
            let mut param_count_u64: u64 = 0;
            let mut count = pluguzuRender(
                self.state,
                start as u64,
                end as u64,
                self.buffer.as_mut_ptr(),
                &mut loc_count_u64,
                self.locs.as_mut_ptr(),
                &mut param_count_u64,
                self.params.as_mut_ptr(),
            ) as usize;

            if count > self.max_event {
                println!("Too many events rendered: {}", count);
                count = self.max_event
            }
            self.buffer.set_len(count);

            let mut loc_count = loc_count_u64 as usize;
            if loc_count > self.max_loc {
                println!("Too many locs rendered: {}", loc_count);
                loc_count = self.max_loc
            }
            self.locs.set_len(loc_count);

            let mut param_count = param_count_u64 as usize;
            if param_count > self.max_param {
                println!("Too many params rendered: {}", param_count);
                param_count = self.max_param
            }
            self.params.set_len(param_count);

            // Collect sound names
            for ev in &self.buffer {
                if ev.sound != 0 {
                    nih_plug::util::permit_alloc(|| {
                        self.sounds.entry(ev.sound).or_insert_with(|| {
                            let c_str = pluguzuSoundName(self.state, ev.sound);
                            if c_str.is_null() {
                                "NA"
                            } else {
                                std::ffi::CStr::from_ptr(c_str).to_str().unwrap()
                            }
                        });
                    })
                }
            }
        };
        self.bar_start = Some(start);
    }
}

#[repr(C)]
struct CPluguzuState {
    _data: (),
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}
unsafe impl Send for PluguzuState {}
