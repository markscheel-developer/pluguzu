# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the DISTRHO Plugin Framework.

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

## Build Instructions

- Install the system libraries and the Haskell toolchain:

```bash
sudo dnf install -y mesa-libGL-devel libXext-devel libXrandr-devel libXfixes-devel dbus-devel gcc gcc-c++ gmp gmp-devel make ncurses ncurses-compat-libs xz perl pkg-config git
curl --proto '=https' --tlsv1.2 -sSf https://get-ghcup.haskell.org | env BOOTSTRAP_HASKELL_GHC_VERSION=latest BOOTSTRAP_HASKELL_NONINTERACTIVE=1 sh
source ~/.ghcup/env
```

- Get the sources:

```
git clone --recursive https://codeberg.org/TristanCacqueray/pluguzu
cd pluguzu
```

- Build the plugin:

```bash
make         # or: make haskell-lib vst2 clap
make install # or: make install-vst2 install-clap
```

- Validate by making sure this command doesn't output a missing dependencies:

```bash
ldd ~/.vst/pluguzu-vst.so | grep "not found"
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
- [x] Handle transport pause/jump.
- [x] Display status and parse error.
- [x] Update text editor language def to support Haskell syntax.
- [x] Add UI/config-file to define custom sound to note mapping, e.g. for "bd", "sd", ...
- [x] Handle MIDI CC
- [ ] Add config-file to define preset pattern to help getting started.
- [ ] Visualize the pattern, e.g. a pianoroll in the background.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
