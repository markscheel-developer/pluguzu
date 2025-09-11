// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::egui::{Pos2, Rect, Vec2};
use crate::VERT_SPACE;
use std::ops::Add;

use std::sync::{Arc, RwLock};

pub struct EditorPixels {
    top_left: Pos2,
    font_size: f32,
    line_height: f32,
}

impl EditorPixels {
    pub fn new(top_left: Pos2, font_size: f32) -> Self {
        EditorPixels {
            top_left,
            font_size: font_size * 0.6,
            line_height: font_size + VERT_SPACE,
        }
    }
    fn char_pos(&self, col: f32, row: f32) -> Pos2 {
        self.top_left.add(Vec2::new(
            3. + col * self.font_size,
            VERT_SPACE + 1. + row * self.line_height,
        ))
    }
    pub fn highlight_rect(&self, col: usize, row: usize, len: usize) -> Rect {
        Rect::from_min_size(
            self.char_pos(col as f32, row as f32),
            Vec2::new(len as f32 * self.font_size, self.line_height),
        )
    }
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct EventLoc {
    pub when: u64,
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

fn get_pos(code: &str, mut start: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    let mut chars = code.chars();
    while start > 0 {
        match chars.next() {
            Some('\n') => {
                col += 1;
                row = 0;
            }
            Some(_) => row += 1,
            None => {
                println!("Oops, code too short?! {start}");
                break;
            }
        }
        start -= 1;
    }
    (row, col)
}

pub fn update_locs(
    evlocs: Arc<RwLock<Vec<EventLoc>>>,
    code: &str,
    mevs: &[crate::binding::MidiEvent],
    locs: &[crate::binding::LocEvent],
) {
    let mut evlocs = evlocs.write().unwrap();
    evlocs.clear();
    for loc in locs {
        if let Some(mev) = mevs.get(loc.pos as usize) {
            let (row, col) = get_pos(code, loc.start as usize);
            evlocs.push(EventLoc {
                when: (mev.start * 1000.0) as u64,
                len: (loc.stop - loc.start) as usize,
                row,
                col,
            })
        }
    }
}
