// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use dough_sys::Source;
use std::ffi::c_int;

pub struct Dough {
    pub sr: f64,
    pos: usize,
    frame_count: usize,
}

impl Dough {
    pub fn new() -> Self {
        Dough {
            sr: 0.,
            pos: 0,
            frame_count: 0,
        }
    }
    pub fn init_buffer(&mut self, frame_count: usize) {
        self.frame_count = frame_count;
        self.pos = 0;
    }
    pub fn initialize(&mut self, sr: f64) {
        if self.sr == 0. {
            unsafe { dough_sys::dough_init(sr as c_int, 64, 2, 3, 0, 0) };
            self.sr = sr;
        } else if self.sr != sr {
            todo!("Sample size change is not supported!")
        }
    }
    pub fn advance(&mut self, until: usize, samples: &mut [&mut [f32]]) {
        let max = until.min(self.frame_count);
        for idx in self.pos..max {
            let mut arr = [0., 0.];
            unsafe { dough_sys::gen_sample(arr.as_mut_ptr(), 0) };
            samples[0][idx] = arr[0];
            samples[1][idx] = arr[1];
        }
        self.pos = max;
    }
    pub fn stop(&self) {
        unsafe { dough_sys::panic() };
    }
}

pub struct Sample {
    _samples: Vec<f32>,
    c: dough_sys::FileSource,
}

use thiserror::Error;
#[derive(Error, Debug)]
pub enum SampleError {
    #[error("couldn't read file")]
    ReadError,

    #[error("invalid sndfile")]
    SoundFile(sndfile::SndFileError),

    #[error("invalid sample rate convertion")]
    SampleRate(#[from] samplerate::Error),
}

impl Sample {
    pub fn from_file(
        path: &std::path::Path,
        freq: f32,
        sample_rate: u32,
    ) -> Result<Sample, SampleError> {
        use sndfile::*;
        let mut snd = sndfile::OpenOptions::ReadOnly(ReadOptions::Auto)
            .from_path(path)
            .map_err(SampleError::SoundFile)?;

        let samples = snd.read_all_to_vec().map_err(|_| SampleError::ReadError)?;
        let samples = samplerate::convert(
            snd.get_samplerate() as u32,
            sample_rate,
            snd.get_channels(),
            samplerate::converter_type::ConverterType::SincBestQuality,
            &samples,
        )
        .map_err(SampleError::SampleRate)?;
        let c = dough_sys::FileSource {
            pcm: samples.as_ptr(),
            frames: (samples.len() / snd.get_channels()) as c_int,
            channels: snd.get_channels() as c_int,
            freq: freq,
            pos: 0.0,
        };
        Ok(Sample {
            _samples: samples,
            c,
        })
    }
}

pub struct Sound(pub(crate) dough_sys::Event);

impl Sound {
    pub fn new() -> Self {
        let mut ev = dough_sys::Event::default();
        unsafe { dough_sys::reset_event(&mut ev) };
        ev.orbit = 0;
        Sound(ev)
    }

    pub fn submit(&mut self) {
        unsafe { dough_sys::process_event(&self.0) };
    }

    pub fn default_sound(&mut self, sound: &str) -> bool {
        let sound = match sound {
            "tri" => Some(Source::TRI_OSC),
            "sine" => Some(Source::SINE_OSC),
            "pulse" => Some(Source::PULSE_OSC),
            "saw" => Some(Source::SAW_OSC),
            _ => None,
        };
        if let Some(sound) = sound {
            self.0.sound = sound;
            true
        } else {
            self.0.sound = Source::FILE_SRC;
            false
        }
    }

    pub fn file_source(&mut self, samples: &Sample) {
        self.0.file_source = samples.c;
    }

    pub fn apply_param(&mut self, name: &str, v: f32) {
        match name {
            "gate" => self.0.gate = v,
            "duration" => self.0.duration = v,
            "midinote" => self.0.freq = midi2freq(v),
            "note" => self.0.freq = midi2freq(v + 72.),
            "speed" => self.0.speed = v,
            "cutoff" => self.0.lpf = v,
            "lpe" => self.0.lpe = v,
            "lpd" => self.0.lpd = v,
            "pw" => self.0.pw = v,
            "resonance" => self.0.lpq = v,
            "hcutoff" => self.0.hpf = v,
            "hresonance" => self.0.hpq = v,
            "bandf" => self.0.bpf = v,
            "bandq" => self.0.bpq = v,
            "gain" => self.0.gain = v,
            "attack" => self.0.attack = v,
            "decay" => self.0.decay = v,
            "sustain" => self.0.sustain = v,
            "release" => self.0.release = v,
            "delay" => self.0.delay = v,
            "delaytime" => self.0.delaytime = v,
            "delayfeedback" => self.0.delayfeedback = v,
            "crush" => self.0.crush = v,
            "coarse" => self.0.coarse = v,
            "distort" => self.0.distort = v,
            "distortvol" => self.0.distortvol = v,
            "vib" => self.0.vib = v,
            "vibmod" => self.0.vibmod = v,
            "pan" => self.0.pan = v,
            _ => {}
        }
    }
}

pub fn midi2freq(midi: f32) -> f32 {
    2.0_f32.powf((midi - 69.0) / 12.0) * 440.0
}

pub const SOUNDS: &'static [&'static str] = &[
    "sine", "triangle", "tri", "saw", "pulse", "white", "pink", "brown",
];

unsafe impl Send for Sample {}
unsafe impl Sync for Sample {}
