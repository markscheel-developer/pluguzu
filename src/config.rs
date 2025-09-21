// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! This module defines user config location.

use nih_plug::nih_warn;
use std::path::PathBuf;

pub struct Config(PathBuf);

impl Config {
    pub fn new() -> Config {
        if let Some(path) = dirs::config_dir().and_then(|mut p| {
            p.push("pluguzu");
            match std::fs::create_dir_all(&p) {
                Err(err) => {
                    nih_warn!("{:?}: Couldn't create config dir: {err}", p);
                    None
                }
                Ok(_) => Some(p),
            }
        }) {
            Config(path)
        } else {
            Config("/pluguzu".to_string().into())
        }
    }
    pub fn path(&self, path: &str) -> PathBuf {
        let mut p = self.0.clone();
        p.push(path);
        p
    }
}
