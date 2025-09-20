// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use smol_str::SmolStr;
use std::collections::HashMap;
use std::io::Result;
use std::path::PathBuf;

use crate::events::SoundEvent;
use crate::{dirt, info};

#[derive(Eq, Hash, PartialEq)]
pub struct AbsoluteSoundEvent(SoundEvent);

pub struct SampleLibrary {
    pub sample_rate: u32,
    pub samples: HashMap<AbsoluteSoundEvent, dirt::Sample>,
    pub samples_count: HashMap<SmolStr, Vec<PathBuf>>,
}

impl Default for SampleLibrary {
    fn default() -> Self {
        SampleLibrary {
            sample_rate: 48000,
            samples: HashMap::new(),
            samples_count: HashMap::new(),
        }
    }
}

impl AbsoluteSoundEvent {
    fn from_sev(sev: &SoundEvent, paths: &[PathBuf]) -> Self {
        AbsoluteSoundEvent(SoundEvent {
            name: sev.name.clone(),
            idx: sev.idx % paths.len() as i64,
        })
    }
}

impl SampleLibrary {
    pub fn has_sample(&self, sev: &SoundEvent) -> bool {
        match self.samples_count.get(&sev.name) {
            None => false,
            Some(count) => {
                let asev = AbsoluteSoundEvent::from_sev(sev, count);
                self.samples.contains_key(&asev)
            }
        }
    }
    pub fn get(&self, sev: &SoundEvent) -> Option<&dirt::Sample> {
        self.samples_count
            .get(&sev.name)
            .and_then(|paths| self.samples.get(&AbsoluteSoundEvent::from_sev(sev, paths)))
    }
    pub fn load(&mut self, root: &std::path::Path, sev: &SoundEvent) -> Result<()> {
        if let Some(paths) = self.samples_count.get(&sev.name) {
            load_path(&mut self.samples, self.sample_rate, paths, sev)?;
        } else {
            let mut path = root.to_path_buf();
            path.push(&sev.name);
            let mut samples = Vec::new();
            for elem in std::fs::read_dir(&path)? {
                let elem = elem?;
                if elem.file_type()?.is_file() {
                    samples.push(elem.path())
                }
            }
            let name = sev.name.clone();
            if let Err(e) = load_path(&mut self.samples, self.sample_rate, &samples, sev) {
                self.samples_count.insert(name, samples);
                return Err(e);
            }
            self.samples_count.insert(name, samples);
        }
        Ok(())
    }
}

fn load_path(
    samples: &mut HashMap<AbsoluteSoundEvent, dirt::Sample>,
    sample_rate: u32,
    paths: &[PathBuf],
    sev: &SoundEvent,
) -> Result<()> {
    if paths.is_empty() {
        return Err(std::io::Error::other("empty samples path"));
    }
    let asev = AbsoluteSoundEvent::from_sev(sev, paths);
    let path = paths
        .get(asev.0.idx.unsigned_abs() as usize)
        .expect("Absolute path!");
    info!("Loading {:?} {:?}", path, asev.0);
    let sample = dirt::Sample::from_file(path.as_path(), sample_rate);
    samples.insert(asev, sample);
    Ok(())
}
