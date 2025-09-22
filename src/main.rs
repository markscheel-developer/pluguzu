// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The command line entry point

use pluguzu::haskell::HaskellRuntime;

fn main() {
    let mut args = std::env::args().collect::<Vec<_>>();
    if let Some(cmd) = args.get(1)
        && cmd == "debug"
    {
        let pat = args.get(2).expect("a pattern");
        let mut ps = HaskellRuntime::default();
        if let Some(err) = ps.parse(pat) {
            println!("Parse failure: {err}");
        } else {
            let events = ps.render(0, 4);
            for (nr, ev) in events.iter().enumerate() {
                println!("{nr}=> {ev:?}");
            }
        }
    } else {
        if let Some(path) = args.get(1)
            && !path.starts_with('-')
        {
            if !std::fs::exists(path).unwrap_or(false) {
                std::fs::File::create(path)
                    .unwrap_or_else(|err| panic!("{path}: can't create {err}"));
            }
            // The args will be read when the plugin initialize.
            // We need to remove it from what nih standalone expect.
            args.remove(1);
        }
        nih_plug::prelude::nih_export_standalone_with_args::<pluguzu::Pluguzu, _>(args);
    }
}
