// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

macro_rules! debug {
    ($($rest:tt)+) => {
        #[cfg(feature = "debug")]
        println!($($rest)*)
    }
}

pub(crate) use debug;

macro_rules! pwarn {
    ($($rest:tt)+) => {
        nih_plug::util::permit_alloc(|| {
            nih_plug::nih_warn!($($rest)*)
        })
    }
}

pub(crate) use pwarn;

macro_rules! info {
    ($($rest:tt)+) => {
        nih_plug::nih_log!($($rest)*)
    }
}

pub(crate) use info;
