// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This modules defines a library of samples

use dashmap::{mapref::one::Ref, DashMap};
use smol_str::SmolStr;
use std::io::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::events::SoundEvent;
use crate::{dirt, info, pwarn};

#[derive(Eq, Hash, PartialEq)]
pub struct AbsoluteSoundEvent(SoundEvent);

pub struct SampleLibrary {
    search_path: PathBuf,
    sample_rate: AtomicU32,
    pub samples: DashMap<AbsoluteSoundEvent, dirt::Sample>,
    pub samples_count: DashMap<SmolStr, Vec<PathBuf>>,
}

impl AbsoluteSoundEvent {
    fn from_sev(sev: &SoundEvent, paths: &[PathBuf]) -> Self {
        let idx = if paths.is_empty() {
            pwarn!("Empty paths shall not be possible!");
            0
        } else {
            sev.idx % paths.len() as i64
        };
        AbsoluteSoundEvent(SoundEvent {
            name: sev.name.clone(),
            idx,
        })
    }
}

impl SampleLibrary {
    pub fn new(search_path: PathBuf) -> Self {
        SampleLibrary {
            search_path,
            sample_rate: AtomicU32::new(48000),
            samples: DashMap::new(),
            samples_count: DashMap::new(),
        }
    }
    pub fn set_sample_rate(&self, sr: u32) {
        self.sample_rate.store(sr, Ordering::Relaxed);
    }

    pub fn has_sample(&self, sev: &SoundEvent) -> bool {
        match self.samples_count.get(&sev.name) {
            None => false,
            Some(count) => {
                let asev = AbsoluteSoundEvent::from_sev(sev, &count);
                self.samples.contains_key(&asev)
            }
        }
    }
    pub fn get(&self, sev: &SoundEvent) -> Option<Ref<'_, AbsoluteSoundEvent, dirt::Sample>> {
        self.samples_count
            .get(&sev.name)
            .and_then(|paths| self.samples.get(&AbsoluteSoundEvent::from_sev(sev, &paths)))
    }
    pub fn load(&self, sev: &SoundEvent) -> Result<()> {
        let sr = self.sample_rate.load(Ordering::Relaxed);
        if let Some(paths) = self.samples_count.get(&sev.name) {
            load_path(&self.samples, sr, &paths, sev)?;
        } else {
            let mut path = self.search_path.to_path_buf();
            path.push(&sev.name);
            let paths = read_files(&path)
                .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
            let name = sev.name.clone();
            load_path(&self.samples, sr, &paths, sev)?;
            self.samples_count.insert(name, paths);
        }
        Ok(())
    }
}

fn load_path(
    samples: &DashMap<AbsoluteSoundEvent, dirt::Sample>,
    sample_rate: u32,
    paths: &[PathBuf],
    sev: &SoundEvent,
) -> Result<()> {
    if paths.is_empty() {
        panic!("The impossible have happened, paths should be non empty");
    };
    let asev = AbsoluteSoundEvent::from_sev(sev, paths);
    let path = paths
        .get(asev.0.idx.unsigned_abs() as usize)
        .expect("Absolute path!");
    info!("Loading {:?} {:?}", path, asev.0);
    let sample = dirt::Sample::from_file(path.as_path(), sample_rate)
        .map_err(|err| std::io::Error::other(format!("{}: {err}", path.display())))?;
    samples.insert(asev, sample);
    Ok(())
}

fn read_files(path: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut samples = Vec::new();
    for elem in std::fs::read_dir(path)? {
        let elem = elem?;
        if elem.file_type()?.is_file() {
            // TODO: filter non samples
            samples.push(elem.path())
        }
    }
    if samples.is_empty() {
        Err(std::io::Error::other(format!(
            "{}: no files found",
            path.display()
        )))
    } else {
        Ok(samples)
    }
}
