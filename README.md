# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the DISTRHO Plugin Framework.

```ShellSession
cabal build
make
make run
```

## Architecture

- [PluguzuCore.hs](./src/PluguzuCore.hs) contains the logic to convert tidal pattern into MIDI events.
- [Pluguzu.hs](./Pluguzu.hs) is the foreign library called by the plugin.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.
- [PluguzuUI.cpp](./PluguzuUI.cpp) is the plugin component in charge of the editor UI and creating the MIDI events.
- [PluguzuDSP.cpp](./PluguzuDSP.cpp) is the main plugin component in charge of sending the MIDI events.
- [DistrhoPluginInfo.h](./DistrhoPluginInfo.h) describe the plugin settings for the DPF compilation system.

## Roadmap

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [x] Parse tidal pattern from the plugin text editor UI.
- [x] Render the midi events and submit them from the plugin DSP.
- [x] Passthrough midi events.
- [x] Handle transport pause/jump.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
