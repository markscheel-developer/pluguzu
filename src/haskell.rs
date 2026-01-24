// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the Haskell FFI.

use parking_lot::Mutex;
use std::ffi::c_char;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::events::Event;

#[link(name = "pluguzu")]
unsafe extern "C" {
    fn pluguzu_init();
    fn pluguzuNew() -> *mut CHaskellState;
    fn pluguzuParse(state: *mut CHaskellState, mondo: bool, code: *const c_char);
    fn pluguzuError(state: *mut CHaskellState, row: &mut u64, col: &mut u64) -> *const c_char;
    fn pluguzuRender(state: *mut CHaskellState, start: u64, end: u64, count: &mut u64) -> *mut u8;
}

/// The events shared with the audio callback.
pub struct BackgroundBuffer {
    pub stale: Option<Arc<Vec<Event>>>,
    pub events: Arc<Vec<Event>>,
}

impl Default for BackgroundBuffer {
    fn default() -> Self {
        BackgroundBuffer {
            stale: None,
            events: Arc::new(vec![]),
        }
    }
}

/// The Haskell runtime, used to render events and de-allocate past buffer.
/// This shouldn't be used from the audio callback.
pub struct HaskellRuntime(*mut CHaskellState);

impl Default for HaskellRuntime {
    fn default() -> Self {
        HaskellRuntime(unsafe {
            pluguzu_init();
            pluguzuNew()
        })
    }
}

pub struct HaskellState {
    pub rts: Mutex<HaskellRuntime>,
    pub buffer: Mutex<BackgroundBuffer>,
    pub updated: AtomicBool,
}

impl Default for HaskellState {
    fn default() -> Self {
        HaskellState {
            rts: Mutex::new(HaskellRuntime::default()),
            buffer: Mutex::new(BackgroundBuffer::default()),
            updated: AtomicBool::new(false),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Copy, Clone, PartialEq, Hash)]
pub enum UzuKind {
    Tidal,
    Mondo,
}

impl HaskellRuntime {
    pub fn parse(&mut self, kind: UzuKind, code: &str) -> Option<(String, usize, usize)> {
        let mondo = matches!(kind, UzuKind::Mondo);
        match std::ffi::CString::new(code) {
            Ok(code) => unsafe {
                pluguzuParse(self.0, mondo, code.as_ptr() as *const c_char);
                let mut row = 0;
                let mut col = 0;
                let c_str = pluguzuError(self.0, &mut row, &mut col);
                if c_str.is_null() {
                    None
                } else {
                    Some((
                        std::ffi::CStr::from_ptr(c_str)
                            .to_string_lossy()
                            .into_owned(),
                        row as usize,
                        col as usize,
                    ))
                }
            },
            Err(e) => {
                crate::pwarn!("Couldn't convert code to cstring {e}: {code}");
                None
            }
        }
    }

    pub fn render(&mut self, cycle_count: i32) -> Arc<Vec<Event>> {
        let data: &[u8] = unsafe {
            let mut count_u64: u64 = 0;
            let buf = pluguzuRender(self.0, 0, cycle_count as u64, &mut count_u64);
            let count = count_u64 as usize;
            std::slice::from_raw_parts(buf, count)
        };
        Arc::new(serde_json::from_slice(data).unwrap_or_else(|err| {
            crate::pwarn!("FFI decode error {err}: {}", String::from_utf8_lossy(data));
            vec![]
        }))
    }
}

impl BackgroundBuffer {
    pub fn update(&mut self, events: Arc<Vec<Event>>) {
        // de-allocate the unused buffer that the audio callback gave back.
        self.stale.take();
        self.events = events;
    }
}

#[repr(C)]
struct CHaskellState {
    _data: (),
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}
unsafe impl Send for HaskellRuntime {}
