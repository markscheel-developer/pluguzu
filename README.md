# pluguzu

A standalone live coding text editor.

## Overview and scope

The goal of pluguzu is to provide an easy to use environment to perform live code.
The project presently integrates the [Tidal][tidal] code interpreter
and the [Dirt][dirt] sampler so that it can be used out of the box
without configuring external services.

Pluguzu can be used as a regular text editor, connecting directly to the system's
audio and MIDI ports, but it can also be loaded inside a Digital Audio Workstation
(DAW) as a CLAP plugin (or VST3).

Pluguzu is powered by the [nih-plug][nih-plug] audio plugin framework and the
[egui][egui] graphical user interface.

> Checkout the companion post: https://midirus.com/blog/haskell-foreign-library-dpf

[tidal]: https://tidalcycles.org/
[dirt]: https://codeberg.org/uzu/dirt
[nih-plug]: https://github.com/robbert-vdh/nih-plug
[egui]: https://egui.rs

## Supported Platforms

Pluguzu is known to work on the following system:

- [x] Linux x86

… with the following host:

- [x] Carla (VST3), but the windows must not overlap to avoid lag.
- [x] Ardour8 (VST3), select option to "allow plugin to receive keyboard events".
- [x] REAPER (CLAP), select option to "send all keyboard input to plugin".
- [x] BespokeSynth (VST3). Keyboard events for edition doesn't presently work.


## Build Instructions

- Install the system requirements:

```bash
# Install Fedora/CentOS system dependencies
sudo dnf install -y mesa-libGL-devel libXext-devel libXrandr-devel libXfixes-devel dbus-devel libsndfile-devel gcc gcc-c++ gmp gmp-devel make ncurses ncurses-compat-libs xz perl pkg-config git

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
ldd ~/.local/bin/pluguzu                                  | grep "not found"
ldd ~/.clap/pluguzu.clap                                  | grep "not found"
ldd ~/.vst3/pluguzu.vst3/Contents/x86_64-linux/pluguzu.so | grep "not found"
```

- Run the standalone JACK client:

```bash
make run-standalone
```


## Architecture

Here are the important files:

- [PluguzuCore.hs](./src/PluguzuCore.hs) contains the logic to convert tidal pattern into MIDI events.
- [Pluguzu.hs](./Pluguzu.hs) is the foreign library called by the plugin.
- [lib.rs](./src/lib.rs) is the nih-plug implementation.
- [dirt.rs](./src/dirt.rs) is the dirt sampler integration.
- [pluguzu-presets.md](./pluguzu-presets.md) is the preset library.
- [PluguzuWrapper.c](./PluguzuWrapper.c) is the C wrapper to enable calling Haskell from C.

Pluguzu implements a double buffering logic to render the upcoming events in the background:

- See the `Pluguzu::sync_events_buffer` for the realtime part, and,
- `PluguzuState::render` for the background work.

Pluguzu is driven by the host transport and it takes care of performing the MIDI and Sound events.

## Changelog

Implemented:

- [x] Setup Haskell RTS and demonstrate function call from the DAW.
- [x] Parse tidal pattern from the plugin text editor UI.
- [x] Render the midi events and submit them from the plugin DSP.
- [x] Handle transport pause/jump.
- [x] Display status and parse error.
- [x] Update text editor language def to support Haskell syntax.
- [x] Handle MIDI CC.
- [x] Add presets.
- [x] Replace DPF framework with nih-plug.
- [x] Highlight event locations.
- [x] Integrate the dirt sampler.
- [x] Loading code through the command line and file menu.

Next:

- [ ] Support user-defined presets, e.g. in ~/.config/pluguzu/presets.md.
- [ ] Improve error reporting to the UI.
- [ ] Package the plugin dependencies (e.g. the libpluguzu.so and libHSrts) so that it works out of the box.
- [ ] Add pattern visualizations, e.g. a pianoroll in the background.

Future idea:

- [ ] Add standalone transport, e.g. without sync to an external clock.
- [ ] Support other Uzu languages like Mondo.
