# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the DISTRHO Plugin Framework.

```ShellSession
# Compile the plugin in the bin directory.
make haskell-lib vst2 clap
```

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

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
- [x] Display status and parse error.
- [x] Investigate using a dedicated rendering thread with mutex.
- [ ] Update text editor language def to support Haskell syntax.
- [ ] Handle MIDI CC and sound event, for example by mapping common drum machine samples to their usual notes.
- [ ] Presets with popular patterns to help getting started.
- [ ] Global values, for example to share a scale between multiple plugin instance.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
