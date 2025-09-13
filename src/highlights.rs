// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::egui::{Galley, Pos2, Rect, Vec2};
use std::ops::Add;

use std::sync::{Arc, RwLock};

pub struct EditorPixels<'a> {
    top_left: Pos2,
    galley: &'a Galley,
}

impl<'a> EditorPixels<'a> {
    pub fn new(top_left: Pos2, galley: &'a Galley) -> Self {
        EditorPixels { top_left, galley }
    }

    pub fn highlight_rect(&self, col: usize, row: usize, len: usize) -> Rect {
        if let Some(placed_row) = self.galley.rows.get(row) {
            let col_pos = placed_row.x_offset(col);
            let mut end_idx = col + len;
            {
                // Skip trailing white space. TODO: do this processing earlier...
                while end_idx > 0 {
                    if let Some(glyph) = placed_row.glyphs.get(end_idx - 1)
                        && glyph.chr == ' '
                    {
                        end_idx -= 1;
                    } else {
                        break;
                    }
                }
            }
            let end_pos = placed_row.x_offset(end_idx);
            Rect::from_min_size(
                self.top_left
                    .add(placed_row.pos.to_vec2().add(Vec2::new(col_pos, 0.))),
                Vec2::new(end_pos - col_pos, placed_row.height()),
            )
        } else {
            Rect::NOTHING
        }
    }
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Debug)]
pub struct EventLoc {
    pub when: u64,
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

/// Convert the raw event from tidal bindings into EventLoc for faster processing.
pub fn update_locs(
    evlocs: Arc<RwLock<Vec<Arc<EventLoc>>>>,
    mevs: &[crate::binding::MidiEvent],
    locs: &[crate::binding::LocEvent],
) {
    let mut evlocs = evlocs.write().unwrap();
    evlocs.clear();
    for loc in locs {
        if let Some(mev) = mevs.get(loc.pos as usize) {
            evlocs.push(Arc::new(EventLoc {
                when: (mev.start * 1000.0) as u64,
                len: loc.len as usize,
                row: loc.row as usize,
                col: loc.col as usize,
            }));
        }
    }
}
