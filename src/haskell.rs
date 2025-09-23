// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines the Haskell FFI.

use nih_plug::nih_warn;
use std::ffi::c_char;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

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
struct BackgroundBuffer {
    stale: Option<Arc<Vec<Event>>>,
    events: Arc<Vec<Event>>,
    in_place: bool,
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

impl Default for BackgroundBuffer {
    fn default() -> Self {
        BackgroundBuffer {
            stale: None,
            events: Arc::new(vec![]),
            in_place: false,
        }
    }
}

pub struct HaskellState {
    pub rts: Mutex<HaskellRuntime>,
    buffer: Mutex<BackgroundBuffer>,
    bar_start: AtomicI64,
}

impl Default for HaskellState {
    fn default() -> Self {
        HaskellState {
            rts: Mutex::new(HaskellRuntime::default()),
            buffer: Mutex::new(BackgroundBuffer::default()),
            bar_start: AtomicI64::new(i64::MAX),
        }
    }
}

impl HaskellState {
    pub fn load_bar_start(&self) -> Option<i32> {
        let value = self.bar_start.load(Ordering::Relaxed);
        if value == i64::MAX {
            None
        } else {
            Some(value.try_into().expect("bar start overflow!"))
        }
    }
    pub fn store_bar_start(&self, value: Option<i32>) {
        self.bar_start.store(
            value.map(|i| i.into()).unwrap_or(i64::MAX),
            Ordering::Relaxed,
        );
    }
    pub fn lock_read_events(&self, previous: Option<Arc<Vec<Event>>>) -> (bool, Arc<Vec<Event>>) {
        let mut buffer = self.buffer.lock().expect("lock");
        // Here is the tricky part.
        // We must not de-allocate the events buffer in the audio callback.
        if let Some(stale) = buffer.stale.take() {
            // This should never happen because the background thread should always free the buffer after the sync.
            nih_plug::util::permit_alloc(move || {
                nih_warn!("Stale buffer is not free, de-allocating from audio callback...");
                std::mem::drop(stale);
            })
        }
        // So we lend back the forground buffer to avoid dropping it ourself.
        buffer.stale = previous;
        self.store_bar_start(None);
        (buffer.in_place, buffer.events.clone())
    }
    pub fn lock_write_events(
        &self,
        bar_start: i32,
        events: Arc<Vec<Event>>,
        in_place: bool,
    ) -> Arc<Vec<Event>> {
        let (stale, current_events) = {
            let mut buffer = self.buffer.lock().expect("lock");
            buffer.in_place = in_place;
            let current_events = buffer.events.clone();
            buffer.events = events;
            (buffer.stale.take(), current_events)
        };
        self.store_bar_start(Some(bar_start));
        drop(stale);
        current_events
    }
    pub fn lock_render_events(&self, bar_start: i32) -> Arc<Vec<Event>> {
        let mut haskell_runtime = self.rts.lock().expect("lock");
        let next_events = haskell_runtime.render(bar_start, bar_start + crate::BUFFER_LEN);
        let mut buffer = self.buffer.lock().expect("lock");
        buffer.events = next_events.clone();
        buffer.in_place = true;
        buffer.stale.take();
        next_events
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

#[repr(C)]
struct CHaskellState {
    _data: (),
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}
unsafe impl Send for HaskellRuntime {}
