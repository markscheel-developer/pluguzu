// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::ffi::c_int;

// Using https://codeberg.org/uzu/dirt/pulls/87
// run: `make libdirt.so LIB=1` and copy the library to pluguzu bin folder.

pub struct Dirt {
    pub sr: u32,
}

impl Dirt {
    pub fn new(polyphony: usize) -> Self {
        unsafe {
            dirt_sys::audio_init_base(
                dirt_sys::Compressor::Clean,
                false,
                polyphony as c_int,
                false,
            );
        }
        Dirt { sr: 44100 }
    }
    pub fn set_sample_rate(&mut self, sr: u32) {
        unsafe { dirt_sys::set_samplerate(sr as c_int) };
        self.sr = sr;
    }

    pub fn playback(&self, mut now: f64, len: usize, samples: &mut [&mut [f32]]) {
        let sample_rate = self.sr as f64;
        let sample_time = 1. / sample_rate;
        for pos in 0..len {
            let left: *mut f32 = samples[0].as_mut_ptr();
            let right: *mut f32 = samples[1].as_mut_ptr();
            let lr: *mut *mut f32 = [left, right].as_mut_ptr();
            unsafe {
                dirt_sys::playback(lr, pos as c_int, now);
                dirt_sys::dequeue(now);
            }
            now += sample_time;
        }
    }
}

pub struct Sample {
    _samples: Vec<f32>,
    c: dirt_sys::Sample,
}

impl Sample {
    pub fn from_file(path: &std::path::Path, sample_rate: u32) -> Sample {
        use sndfile::*;
        let mut snd = sndfile::OpenOptions::ReadOnly(ReadOptions::Auto)
            .from_path(path)
            .unwrap();
        let samples = snd.read_all_to_vec().unwrap();
        let samples = samplerate::convert(
            snd.get_samplerate() as u32,
            sample_rate,
            snd.get_channels(),
            samplerate::converter_type::ConverterType::SincBestQuality,
            &samples,
        )
        .unwrap();
        let c = dirt_sys::Sample {
            name: [0; 256],
            items: samples.as_ptr(),
            frames: samples.len() as c_int,
            channels: snd.get_channels() as c_int,
            onsets: std::ptr::null(),
        };
        Sample {
            _samples: samples,
            c,
        }
    }
}

pub struct Sound(&'static mut dirt_sys::Sound, u32);

// from Dirt config.h
const CUTOFFRATIO: f32 = 30000.0;
const MAX_ORBIT: i32 = 15;

/// Sound manage the t_sound Dirt struct.
/// The params are applied according to the server.c play_dispatch method
impl Sound {
    pub fn new(sample_rate: u32) -> Option<Self> {
        unsafe {
            let csound = dirt_sys::new_sound();
            if csound.is_null() {
                None
            } else {
                let mut sound = Sound(&mut *csound, sample_rate);
                sound.set_default();
                Some(sound)
            }
        }
    }

    fn filter_freq(&self, v: f32) -> f32 {
        (v / CUTOFFRATIO) * (44100.0 / (self.1 as f32))
    }

    pub fn set_default(&mut self) {
        self.0.active = 1;
        self.0.gain = 1.;
        self.0.velocity = 0.5;
        self.0.delaytime = -1.;
        self.0.delayfeedback = -1.;
        self.0.speed = 1.;
        self.0.unit = unit_num('r');
        self.0.pan = 0.5;
        self.0.attack = -1.;
        self.0.release = -1.;
        self.0.formant_vowelnum = -1;
        // That's a bit odd, in dirt audio.c init_effects(), it seems like resonances must be set.
        // so let's use raisonable value so that 'cutoff' works out of the box:
        self.0.resonance = 0.1;
        self.0.hresonance = 0.1;
        self.0.bandq = 0.1;
    }

    pub fn play(&mut self, start: f64, cps: f32, samples: &Sample) {
        self.0.when = start;
        self.0.cps = cps;
        unsafe {
            dirt_sys::audio_play_with_sample(self.0, &samples.c);
        };
    }

    pub fn apply_param(&mut self, name: &str, v: f32) {
        match name {
            "begin" => self.0.start = v,
            "end" => self.0.end = v,
            "speed" => self.0.speed = v,
            "pan" => self.0.pan = v,
            "velocity" => self.0.velocity = v,
            "cutoff" => self.0.cutoff = self.filter_freq(v),
            "resonance" => self.0.resonance = v,
            "accelerate" => self.0.accelerate = v,
            "shape" => {
                self.0.shape = if v != 0. { 1 } else { 0 };
                let shape = v.abs();
                if shape > 0.99 {
                    self.0.shape_k = 0.99
                } else {
                    self.0.shape_k = shape
                }
            }
            "gain" => self.0.gain = (v / 2.).powf(4.),
            "cut" => self.0.cutgroup = v.round() as i32,
            "delay" => self.0.delay = v,
            "delaytime" => self.0.delaytime = v,
            "delayfeedback" => self.0.delayfeedback = v,
            "crush" => self.0.crush = v.round() as i32,
            "coarse" => self.0.coarse = v.round() as i32,
            "hcutoff" => self.0.hcutoff = self.filter_freq(v),
            "hresonance" => self.0.hresonance = self.filter_freq(v),
            "bandf" => self.0.bandf = v,
            "bandq" => self.0.bandq = v,
            "loop" => self.0.sample_loop = v.round() as i32,
            "attack" => self.0.attack = v,
            "hold" => self.0.hold = v,
            "release" => self.0.release = v,
            "orbit" => self.0.orbit = (v.round() as i32).min(MAX_ORBIT),
            _ => {}
        }
    }
    pub fn apply_param_char(&mut self, name: &str, c: char) {
        match name {
            "vowel" => {
                self.0.formant_vowelnum = match c {
                    'a' | 'A' => 0,
                    'e' | 'E' => 1,
                    'i' | 'I' => 2,
                    'o' | 'O' => 3,
                    'u' | 'U' => 4,
                    _ => -1,
                }
            }
            "unit" => {
                self.0.unit = unit_num(match c {
                    'r' | 'R' => 'r',
                    's' | 'S' => 's',
                    'c' | 'C' => 'c',
                    _ => 'r',
                })
            }
            _ => {}
        }
    }
}

fn unit_num(c: char) -> i8 {
    u8::try_from(c).unwrap() as i8
}

unsafe impl Send for Sample {}
unsafe impl Sync for Sample {}
unsafe impl Send for Sound {}
