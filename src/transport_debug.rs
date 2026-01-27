// Copyright © 2026 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! A test plugin to debug the cycle transport computation.

use crate::transport::CycleTransport;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Default)]
pub struct DebugTransport {
    info: CycleTransport,
    last: Option<crate::transport::CycleRange>,
}

#[derive(Params)]
struct DParams {
    _a: (),
}

impl Plugin for DebugTransport {
    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.info.sample_rate = buffer_config.sample_rate as f64;
        true
    }
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Lookup external transport info.
        let transport = context.transport();
        let buffer_length = buffer.samples() as i64;
        if let Some(range) = self.info.update_transport(buffer_length, transport) {
            if range.jumped {
                crate::pwarn!("Jumped to {}", range.arc);
            }
            if Some(range) != self.last {
                let pos = transport.pos_samples().unwrap_or(0);
                crate::pwarn!(
                    "{}  (transport{{sample: {:.7}, time: {:.7}}})",
                    range.arc,
                    pos,
                    transport.pos_seconds().unwrap_or(-42.)
                )
            }
            self.last = Some(range)
        } else {
            crate::pwarn!("Unknown transport?! {:?}", transport)
        };
        ProcessStatus::KeepAlive
    }
    fn params(&self) -> Arc<dyn Params> {
        std::sync::Arc::new(DParams { _a: () })
    }

    type BackgroundTask = ();

    type SysExMessage = ();

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            // This is also the default and can be omitted here
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: None,
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::MidiCCs;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    const NAME: &'static str = "pluguzu-debug-transport";
    const VENDOR: &'static str = "Midirus";
    const URL: &'static str = "https://codeberg.org/TristanCacqueray/pluguzu";
    const EMAIL: &'static str = "tristan@midirus.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
}

impl ClapPlugin for DebugTransport {
    const CLAP_ID: &'static str = "com.midirus.pluguzu-debug-transport";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("uzu-lang live editor");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::NoteEffect, ClapFeature::Utility];
}

impl Vst3Plugin for DebugTransport {
    const VST3_CLASS_ID: [u8; 16] = *b"PluguzuYeahBoyyy";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Generator];
}

// must comment the lib.rs export before building this plugin
// nih_export_clap!(DebugTransport);
// nih_export_vst3!(DebugTransport);
