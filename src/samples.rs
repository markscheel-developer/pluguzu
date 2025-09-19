// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::dirt;
use std::collections::HashMap;
use std::io::Result;
use std::path::PathBuf;

#[derive(Ord, Eq, PartialOrd, PartialEq, Hash, Debug)]
pub struct SampleID {
    pub name: i32,
    pub idx: u8,
}

pub struct SampleLibrary {
    pub sample_rate: u32,
    pub samples: HashMap<crate::samples::SampleID, dirt::Sample>,
    pub samples_count: HashMap<i32, Vec<PathBuf>>,
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

impl SampleLibrary {
    pub fn load(&mut self, root: &std::path::Path, name: &str, sample_id: SampleID) -> Result<()> {
        if let Some(paths) = self.samples_count.get(&sample_id.name) {
            load_path(&mut self.samples, self.sample_rate, paths, sample_id)?;
        } else {
            let mut path = root.to_path_buf();
            path.push(name);
            let mut samples = Vec::new();
            for elem in std::fs::read_dir(&path)? {
                let elem = elem?;
                if elem.file_type()?.is_file() {
                    samples.push(elem.path())
                }
            }
            let name = sample_id.name;
            if let Err(e) = load_path(&mut self.samples, self.sample_rate, &samples, sample_id) {
                self.samples_count.insert(name, samples);
                return Err(e);
            }
            self.samples_count.insert(name, samples);
        }
        Ok(())
    }
}

fn load_path(
    samples: &mut HashMap<crate::samples::SampleID, dirt::Sample>,
    sample_rate: u32,
    paths: &[PathBuf],
    sample_id: SampleID,
) -> Result<()> {
    if paths.is_empty() {
        return Err(std::io::Error::other("empty samples path"));
    }
    let path_idx = sample_id.idx as usize % paths.len();
    let path = paths.get(path_idx).unwrap();
    let sample_id = SampleID {
        name: sample_id.name,
        idx: path_idx
            .try_into()
            .expect("Convertion back to u8 should work"),
    };
    println!("Loading {:?} {:?}", path, sample_id);
    let sample = dirt::Sample::from_file(path.as_path(), sample_rate);
    samples.insert(sample_id, sample);
    Ok(())
}
