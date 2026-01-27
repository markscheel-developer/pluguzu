// Copyright © 2026 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module contains the transport logic for the tidal cycles.
// checkout the transport_debug.rs module for a standalone demo of this transport.

/// Keep track of the current arc.
#[derive(Debug, Copy, Clone)]
pub struct CycleTransport {
    previous_end: Option<f64>,
    pub sample_rate: f64,
}

impl Default for CycleTransport {
    fn default() -> Self {
        CycleTransport {
            previous_end: None,
            sample_rate: 48000.,
        }
    }
}

/// A cycle arc defines the current process callback run start..end position in the tidal cycles.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CycleArc {
    pub start: f64,
    pub end: f64,
}

impl std::fmt::Display for CycleArc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "arc[{:.7}..{:.7}]", self.start, self.end)
    }
}

/// A cycle range is computer for every process callback run.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CycleRange {
    // Jumped indicate that the current range is not continuous with the last run.
    pub jumped: bool,
    pub frames_per_cycle: f64,
    pub cycle_length_secs: f64,
    pub arc: CycleArc,
}

// Note [cycle jitter tolerance]
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// in most situation, the next arc.start precisely match the last arc.end,
// however the time calculation may have some variations and we must tolerate
// at least 1e-6 (micro cycle) jitter.
//
// however, when changing the play rate in a sequencer like REAPER, the jitter
// might increase a lot, and to avoid jumping in the tidal cycle, we tolerate
// a much greater gap.
//
// the downside is that seeking within the TOLERANCE (while playing) might skip
// some events.
const TOLERANCE: f64 = 0.7;

impl CycleTransport {
    pub fn update_transport(
        &mut self,
        frames_count: i64,
        transport: &nih_plug::prelude::Transport,
    ) -> Option<CycleRange> {
        transport.pos_beats().and_then(|pos_beats| {
            transport.tempo.map(|tempo| {
                let sig_numerator = transport.time_sig_numerator.unwrap_or(4);
                self.update_local(
                    transport.playing,
                    frames_count,
                    pos_beats,
                    tempo,
                    sig_numerator,
                )
            })
        })
    }
    pub fn update_local(
        &mut self,
        playing: bool,
        frames_count: i64,
        pos_beats: f64,
        tempo: f64,
        sig_numerator: i32,
    ) -> CycleRange {
        let buffer_length_secs = frames_count as f64 / self.sample_rate;
        let cycle_length_secs = (60 * sig_numerator) as f64 / tempo;
        let start = pos_beats / (sig_numerator as f64);
        let end = start + buffer_length_secs / cycle_length_secs;
        let buffer_length_cycles = buffer_length_secs / cycle_length_secs;
        let frames_per_cycle = frames_count as f64 / buffer_length_cycles;

        if playing {
            #[rustfmt::skip]
            // crate::debug!("{:.9} HERE: {}..{} prev ({:?})", pos_beats, start, end, self.previous_end);
            let continuous_start = if let Some(previous_end) = self.previous_end
                && end > previous_end // when fast restarting the transport, the range can get inverted
                && (start - previous_end).abs() < TOLERANCE
            {
                Some(previous_end)
            } else {
                None
            };
            self.previous_end = Some(end);
            CycleRange {
                frames_per_cycle,
                cycle_length_secs,
                jumped: continuous_start.is_none(),
                arc: CycleArc {
                    start: continuous_start.unwrap_or(start),
                    end,
                },
            }
        } else {
            self.previous_end = None;
            CycleRange {
                frames_per_cycle,
                cycle_length_secs,
                jumped: false,
                arc: CycleArc { start, end },
            }
        }
    }
}
