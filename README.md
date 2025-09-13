# pluguzu

A PoC uzu-lang running as CLAP plugin (or VST) through the nih-plug framework.

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

## Supported Platforms

Pluguzu is known to work on the following system:

- [x] Linux x86

… with the following host:

- [x] Carla (VST3), but the windows must not overlap to avoid lag
- [x] Ardour8 (VST3), select option to "allow plugin to receive keyboard events".
- [x] REAPER (CLAP), select option to "send all keyboard input to plugin".
- [x] BespokeSynth (VST3). Keyboard events for edition doesn't presently work, only presets can be used.


## Build Instructions

- Install the system requirements:

```bash
# Install Fedora/CentOS system dependencies
sudo dnf install -y mesa-libGL-devel libXext-devel libXrandr-devel libXfixes-devel dbus-devel gcc gcc-c++ gmp gmp-devel make ncurses ncurses-compat-libs xz perl pkg-config git

# Install Haskell cabal
curl --proto '=https' --tlsv1.2 -sSf https://get-ghcup.haskell.org | env BOOTSTRAP_HASKELL_GHC_VERSION=latest BOOTSTRAP_HASKELL_NONINTERACTIVE=1 sh
source ~/.ghcup/env

# Install Rust cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

- Get the sources:

```
git clone https://codeberg.org/TristanCacqueray/pluguzu
cd pluguzu
```

- Build the plugin:

```bash
make
make install
```

- Validate by making sure this command doesn't output a missing dependencies:

```bash
ldd ~/.vst3/pluguzu.vst3/Contents/x86_64-linux/pluguzu.so | grep "not found"
ldd ~/.clap/pluguzu.clap                                  | grep "not found"
```

- Run the standalone JACK client:

```bash
make run-standalone
```

## Architecture

- [PluguzuCore.hs](./src/PluguzuCore.hs) contains the logic to convert tidal pattern into MIDI events.
- [Pluguzu.hs](./Pluguzu.hs) is the foreign library called by the plugin.
- [lib.rs](./src/lib.rs) is the nih-plug implementation.
- [pluguzu-presets.md](./pluguzu-presets.md) is the preset library.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.

## Changelog

Implemented:

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [x] Parse tidal pattern from the plugin text editor UI.
- [x] Render the midi events and submit them from the plugin DSP.
- [x] Handle transport pause/jump.
- [x] Display status and parse error.
- [x] Update text editor language def to support Haskell syntax.
- [x] Handle MIDI CC
- [x] Add config-file to define preset pattern to help getting started.
- [x] Highlight event locations
- [x] Replace DPF framework with nih-plug

Next:

- [ ] Add UI/config-file to define custom sound to note mapping, e.g. for "bd", "sd", ...
- [ ] Support user-defined presets, e.g. in ~/.config/pluguzu/presets.md
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
- [ ] Add pattern visualizations, e.g. a pianoroll in the background.
- [ ] Add standalone transport, e.g. without sync to an external clock

Future idea:

- [ ] Integrate the dirt sampler
