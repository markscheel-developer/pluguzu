// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use std::ffi::{c_char, c_int, c_uint, c_void};

// Using https://codeberg.org/uzu/dirt/pulls/87
// run: `make libdirt.so LIB=1` and copy the library to pluguzu bin folder.

#[link(name = "dirt")]
unsafe extern "C" {
    fn new_sound() -> *mut CSound;
    fn audio_play_with_sample(sound: *mut CSound, samples: *const CSample);
    fn audio_init_base(
        compressor: Compressor,
        late_trigger: bool,
        polyphony: c_int,
        shape_gain_comp: bool,
    );
    fn playback(buffers: *mut *mut f32, frame: c_int, now: f64);
    fn dequeue(now: f64);
    fn set_samplerate(sr: c_int);
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CSample {
    pub name: [u8; 256],
    pub info: *const sndfile_sys::SF_INFO,
    pub items: *const f32,
    onsets: *const i32,
}

#[allow(dead_code)]
#[repr(C)]
pub enum Compressor {
    None,
    Dirty,
    Clean,
    Dave,
    All,
}

pub struct Dirt {
    sr: u32,
}

impl Dirt {
    pub fn new(polyphony: usize) -> Self {
        unsafe {
            audio_init_base(Compressor::Clean, false, polyphony as c_int, false);
        }
        Dirt { sr: 44100 }
    }
    pub fn set_sample_rate(&mut self, sr: u32) {
        unsafe { set_samplerate(sr as c_int) };
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
                playback(lr, pos as c_int, now);
                dequeue(now);
            }
            now += sample_time;
        }
    }
}

pub struct Sample {
    _samples: Vec<f32>,
    _info: Box<sndfile_sys::SF_INFO>,
    c: CSample,
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
        let info = Box::new(sndfile_sys::SF_INFO {
            frames: samples.len() as i64,
            samplerate: sample_rate as i32,
            channels: snd.get_channels() as i32,
            format: 0,   // c_int,
            sections: 0, // c_int,
            seekable: 0, // c_int,
        });
        let c = CSample {
            name: [0; 256],
            items: samples.as_ptr(),
            info: &*info,
            onsets: std::ptr::null(),
        };
        println!("{path:?}: Loaded sample! {:p} {:p}", c.info, &info);
        Sample {
            _samples: samples,
            _info: info,
            c,
        }
    }
}

pub struct Sound(&'static mut CSound);

impl Sound {
    pub fn new() -> Option<Self> {
        unsafe {
            let csound = new_sound();
            if csound.is_null() {
                None
            } else {
                let mut sound = Sound(&mut *csound);
                sound.set_default();
                Some(sound)
            }
        }
    }

    pub fn set_default(&mut self) {
        // The defaults set by the server.c
        self.0.gain = 1.;
        self.0.velocity = 0.5;
        self.0.delaytime = -1.;
        self.0.delayfeedback = -1.;
        self.0.speed = 1.;
        self.0.unit = u8::try_from('r').unwrap() as i8;
        self.0.pan = 0.5;
        self.0.attack = -1.;
        self.0.release = -1.;
        self.0.formant_vowelnum = -1;
    }

    pub fn play(&mut self, now: f64, samples: &Sample) {
        self.0.start_t = now;
        unsafe {
            audio_play_with_sample(self.0, &samples.c);
        };
    }

    pub fn apply_param(&mut self, id: u8, v: f32) {
        // Match the order with PluguzuCore.staticParams
        println!("Applying param {id} => {v}");
        match id {
            0 => self.0.start = v,
            1 => self.0.end = v,
            2 => self.0.speed = v,
            _ => panic!("Unknown param id: {id}, value {v}"),
        }
    }
}

// Keep in sync with dirt/audio.h t_node definition
// cat audio.h | sed -e 's/unsigned int/c_uint/'  -e 's/ *\([^ ]*\)  *\(.*\);/ \2: \1,/' -e 's/ int/ c_int/' -e 's/float/f32/' -e 's/double/f64/' -e 's/sampletime_t/f64/'
#[repr(C)]
struct CSound {
    active: c_int,
    is_playing: c_int,
    start_t: f64,
    samplename: [u8; 257],
    is_loop: c_int,
    sample: *const CSample,
    loop_start: c_uint,
    channels: c_int,
    items: *const f32,
    next: *const c_void,
    prev: *const c_void,
    position: f32,
    speed: f32,
    reverse: c_int,
    pan: f32,
    offset: f32,
    start: f32,
    end: f32,
    velocity: f32,
    formant_vowelnum: c_int,
    cutoff: f32,
    resonance: f32,
    accelerate: f32,
    shape: c_int,
    shape_k: f32,
    kriole_chunk: c_int,
    is_kriole: c_int,
    started: c_int,
    checks: c_int,
    delay: f32,
    delaytime: f32,
    delayfeedback: f32,
    gain: f32,
    cutgroup: c_int,
    mono: c_int,
    crush: c_int,
    crush_bits: f32,
    crush_range: f32,
    coarse: c_int,
    hcutoff: f32,
    hresonance: f32,
    bandf: f32,
    bandq: f32,
    sample_loop: c_int,
    cut_continue: c_int,
    unit: c_char,
    cps: f32,
    when: f64,
    attack: f32,
    hold: f32,
    release: f32,
    playtime: f32,
    orbit: c_int,
    played: c_int,
}

unsafe impl Send for Sample {}
unsafe impl Sync for Sample {}
unsafe impl Send for Sound {}
