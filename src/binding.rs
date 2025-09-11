// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::ffi::{c_char, c_double, c_long, c_ulong};

#[link(name = "pluguzu")]
unsafe extern "C" {
    fn pluguzu_init();
    fn pluguzuNew(size: c_ulong) -> *mut CPluguzuState;
    fn pluguzuParse(state: *mut CPluguzuState, code: *const c_char);
    fn pluguzuError(state: *mut CPluguzuState) -> *const c_char;
    fn pluguzuRender(
        state: *mut CPluguzuState,
        start: c_ulong,
        end: c_ulong,
        buf: *mut MidiEvent,
        locCount: &mut c_ulong,
        loc: *mut LocEvent,
    ) -> c_ulong;
}

pub struct PluguzuState {
    size: usize,
    state: *mut CPluguzuState,
    pub buffer: Vec<MidiEvent>,
    pub locs: Vec<LocEvent>,
    pub bar_start: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MidiEvent {
    pub start: c_double,
    pub stop: c_double,
    pub data: [u8; 3],
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct LocEvent {
    pub pos: c_long,
    pub start: c_long,
    pub stop: c_long,
}

impl PluguzuState {
    pub fn new(size: usize) -> Self {
        let state = unsafe {
            pluguzu_init();
            pluguzuNew(size as u64)
        };
        PluguzuState {
            size,
            state,
            buffer: Vec::with_capacity(size),
            locs: Vec::with_capacity(size * 2),
            bar_start: None,
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
            let mut loc_count: u64 = 0;
            let mut count = pluguzuRender(
                self.state,
                start as u64,
                end as u64,
                self.buffer.as_mut_ptr(),
                &mut loc_count,
                self.locs.as_mut_ptr(),
            ) as usize;
            if count > self.size {
                println!("Too many events rendered: {}", count);
                count = self.size
            }
            self.buffer.set_len(count);
            self.locs.set_len(loc_count as usize);
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
