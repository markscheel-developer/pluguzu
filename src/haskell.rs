// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the Haskell FFI.

use std::ffi::c_char;
use std::sync::Arc;

use crate::events::Event;

#[link(name = "pluguzu")]
unsafe extern "C" {
    fn pluguzu_init();
    fn pluguzuNew() -> *mut CHaskellState;
    fn pluguzuParse(state: *mut CHaskellState, code: *const c_char);
    fn pluguzuError(state: *mut CHaskellState) -> *const c_char;
    fn pluguzuRender(state: *mut CHaskellState, start: u64, end: u64, count: &mut u64) -> *mut u8;
}

/// The events shared with the audio callback.
pub struct HaskellState {
    pub stale: Option<Arc<Vec<Event>>>,
    pub events: Arc<Vec<Event>>,
    pub bar_start: Option<i32>,
    pub in_place: bool,
}

/// The Haskell runtime, used to render events and de-allocate past buffer.
/// This shouldn't be used from the audio callback.
pub struct HaskellRuntime {
    c_state: *mut CHaskellState,
    stale: Option<Arc<Vec<Event>>>,
}

impl Default for HaskellRuntime {
    fn default() -> Self {
        HaskellRuntime {
            c_state: unsafe {
                pluguzu_init();
                pluguzuNew()
            },
            stale: None,
        }
    }
}

impl Default for HaskellState {
    fn default() -> Self {
        HaskellState {
            stale: None,
            events: Arc::new(vec![]),
            bar_start: None,
            in_place: false,
        }
    }
}

impl HaskellRuntime {
    pub fn parse(&mut self, code: &str) -> Option<String> {
        match std::ffi::CString::new(code) {
            Ok(code) => unsafe {
                pluguzuParse(self.c_state, code.as_ptr() as *const c_char);
                let c_str = pluguzuError(self.c_state);
                if c_str.is_null() {
                    None
                } else {
                    Some(
                        std::ffi::CStr::from_ptr(c_str)
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
            },
            Err(e) => {
                crate::pwarn!("Couldn't convert code to cstring {e}: {code}");
                None
            }
        }
    }

    pub fn render(&mut self, start: i32, end: i32) -> Arc<Vec<Event>> {
        let data: &[u8] = unsafe {
            let mut count_u64: u64 = 0;
            let buf = pluguzuRender(self.c_state, start as u64, end as u64, &mut count_u64);
            let count = count_u64 as usize;
            std::slice::from_raw_parts(buf, count)
        };
        // de-allocate the unused buffer that the audio callback gave back.
        self.stale.take();
        Arc::new(serde_json::from_slice(data).unwrap_or_else(|err| {
            crate::pwarn!("FFI decode error {err}: {}", String::from_utf8_lossy(data));
            vec![]
        }))
    }
}

impl HaskellState {
    pub fn update(
        &mut self,
        start: i32,
        events: Arc<Vec<Event>>,
        runtime: &mut HaskellRuntime,
        in_place: bool,
    ) {
        self.bar_start = Some(start);
        // lend back to the runtime thread the past events for de-allocation.
        runtime.stale = self.stale.take();
        self.events = events;
        self.in_place = in_place;
    }
}

#[repr(C)]
struct CHaskellState {
    _data: (),
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}
unsafe impl Send for HaskellRuntime {}
