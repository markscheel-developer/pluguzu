# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the DISTRHO Plugin Framework.

```ShellSession
cabal build
make
make run
```

## Architecture

- [PluguzuCore.hs](./src/PluguzuCore.hs) contains the logic to convert tidal pattern into MIDI events.
- [Pluguzu.hs](./src/Pluguzu.hs) is the foreign library called by the plugin.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.
- [PluguzuUI.cpp](./PluguzuUI.cpp) is the plugin component in charge of the editor UI and creating the MIDI events.
- [PluguzuDSP.cpp](./PluguzuDSP.cpp) is the main plugin component in charge of sending the MIDI events.
- [DistrhoPluginInfo.h](./DistrhoPluginInfo.h) describe the plugin settings for the DPF compilation system.

## Roadmap

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [ ] FFI bindings for [sendNote](https://distrho.github.io/DPF/classUI.html#a9d630b2b1696e01015d869a5b434eb6e).
- [ ] FFI bindings for [TimePosition](https://distrho.github.io/DPF/structTimePosition_1_1BarBeatTick.html) and demonstrate sending a note at the begining of a measure.
- [ ] Implement the `run` callback using the [Waking up Haskell threads from C](https://downloads.haskell.org/ghc/9.12.2/docs/users_guide/exts/ffi.html#waking-up-haskell-threads-from-c) doc.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.

Assuming the above goes well:

- [ ] Integrate a standalone mini notation parser/evaluator, e.g. using Tidal or one of the 'New wave' uzulangs, perhaps through the mondo notation.
- [ ] And/or integrate a Haskell interpreter and feature the full Tidal syntax, e.g. with `$` and `#`.
