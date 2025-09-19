// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0
#![allow(dead_code)]

use nih_plug::nih_warn;
use std::path::PathBuf;

struct ConfigFiles {
    presets: PathBuf,
    samples: PathBuf,
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
                        presets.push("presets.md");
                        let mut samples = p;
                        samples.push("samples");
                        Some(ConfigFiles { presets, samples })
                    }
                }
            }),
        }
    }
    pub fn samples_dir(&self) -> &PathBuf {
        match &self.files {
            Some(cf) => &cf.samples,
            None => panic!("Add support for global samples path"),
        }
    }
}
