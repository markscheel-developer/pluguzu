// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use nih_plug::nih_warn;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::SoundMap;

struct ConfigFiles {
    sound_map: PathBuf,
    _presets: PathBuf,
}

pub struct Config {
    files: Option<ConfigFiles>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            files: dirs::config_dir().and_then(|mut p| {
                p.push("pluguzu");
                match std::fs::create_dir_all(&p) {
                    Err(err) => {
                        nih_warn!("{:?}: Couldn't create config dir: {err}", p);
                        None
                    }
                    Ok(_) => {
                        let mut presets = p.clone();
                        let mut sound_map = p;
                        sound_map.push("sound-map.json");
                        presets.push("presets.md");
                        Some(ConfigFiles {
                            sound_map,
                            _presets: presets,
                        })
                    }
                }
            }),
        }
    }
    pub fn load_sound_map(&self, sound_map: &mut SoundMap) {
        if let Some(prev_map) = self.files.as_ref().and_then(|files| {
            if files.sound_map.exists() {
                match read_user_sound_map(&files.sound_map) {
                    Ok(m) => Some(m),
                    Err(err) => {
                        nih_warn!("{:?}: Couldn't read {err}", files.sound_map);
                        None
                    }
                }
            } else {
                None
            }
        }) {
            for (k, v) in prev_map.into_iter() {
                sound_map.entry(k).or_insert(v);
            }
        } else {
            // Defaults from https://x42-plugins.com/x42/x42-avldrums
            for (k, v) in [
                ("bd", 36),
                ("sd", 37),
                ("clap", 39),
                ("lt", 41),
                ("hh", 42),
                ("oh", 46),
                ("cb", 56),
            ] {
                sound_map.entry(k.into()).or_insert(v);
            }
        }
    }
    pub fn save_sound_map(&self, sound_map: &SoundMap) {
        if let Some(err) = self.files.as_ref().and_then(|files| {
            match save_user_sound_map(&files.sound_map, sound_map) {
                Ok(_) => None,
                Err(err) => Some(format!("{:?}: Couldn't save {err}", files.sound_map)),
            }
        }) {
            nih_warn!("{}", err)
        }
    }
}

fn save_user_sound_map(path: &Path, sound_map: &SoundMap) -> Result<(), Box<dyn Error>> {
    serde_json::to_writer(File::create(path)?, sound_map)?;
    Ok(())
}

fn read_user_sound_map(path: &Path) -> Result<SoundMap, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);
    Ok(serde_json::from_reader(reader)?)
}
