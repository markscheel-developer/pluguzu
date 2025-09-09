# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the nih-plug framework.

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

## Supported Platforms

Pluguzu is known to work on the following system:

- [x] Linux x86

… with the following host:

- [x] Carla (VST2/VST3), but the windows must not overlap to avoid lag
- [x] Ardour8 (VST3), select option to "allow plugin to receive keyboard events".
- [x] REAPER (CLAP), select option to "send all keyboard input to plugin".
- [x] BespokeSynth (VST3). Keyboard events for edition doesn't presently work, only presets can be used.


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
- [pluguzu-presets.md](./pluguzu-presets.md) is the preset library.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.

## Roadmap

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [x] Parse tidal pattern from the plugin text editor UI.
- [x] Render the midi events and submit them from the plugin DSP.
- [x] Handle transport pause/jump.
- [x] Display status and parse error.
- [x] Update text editor language def to support Haskell syntax.
- [x] Add UI/config-file to define custom sound to note mapping, e.g. for "bd", "sd", ...
- [x] Handle MIDI CC
- [x] Add config-file to define preset pattern to help getting started.
- [ ] Visualize the pattern, e.g. a pianoroll in the background.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
