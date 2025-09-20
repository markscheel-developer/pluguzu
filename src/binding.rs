// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::collections::BTreeMap;
use std::ffi::c_char;
use std::sync::Arc;

use crate::events::Event;

#[link(name = "pluguzu")]
unsafe extern "C" {
    fn pluguzu_init();
    fn pluguzuNew() -> *mut CPluguzuState;
    fn pluguzuParse(state: *mut CPluguzuState, code: *const c_char);
    fn pluguzuError(state: *mut CPluguzuState) -> *const c_char;
    fn pluguzuRender(state: *mut CPluguzuState, start: u64, end: u64, count: &mut u64) -> *mut u8;
}

pub struct PluguzuState {
    state: *mut CPluguzuState,
    pub stale: Option<Arc<Vec<Event>>>,
    pub events: Arc<Vec<Event>>,
    pub bar_start: Option<i32>,
    pub sounds: BTreeMap<i32, &'static str>,
}

impl Default for PluguzuState {
    fn default() -> Self {
        let state = unsafe {
            pluguzu_init();
            pluguzuNew()
        };
        PluguzuState {
            state,
            stale: None,
            events: Arc::new(vec![]),
            bar_start: None,
            sounds: BTreeMap::new(),
        }
    }
}

impl PluguzuState {
    pub fn parse(&mut self, code: &str) -> Option<String> {
        match std::ffi::CString::new(code) {
            Ok(code) => unsafe {
                pluguzuParse(self.state, code.as_ptr() as *const c_char);
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
            },
            Err(e) => {
                crate::pwarn!("Couldn't convert code to cstring {e}: {code}");
                None
            }
        }
    }

    pub fn render(&mut self, start: i32, end: i32) {
        let data: &[u8] = unsafe {
            let mut count_u64: u64 = 0;
            let buf = pluguzuRender(self.state, start as u64, end as u64, &mut count_u64);
            let count = count_u64 as usize;
            std::slice::from_raw_parts(buf, count)
        };
        self.stale.take();
        self.events = Arc::new(serde_json::from_slice(data).expect("JSON"));
        self.bar_start = Some(start);
    }
}

#[repr(C)]
struct CPluguzuState {
    _data: (),
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}
unsafe impl Send for PluguzuState {}
