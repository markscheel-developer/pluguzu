#!/usr/bin/make -f

NAME = pluguzu

# --------------------------------------------------------------
# Files to build

FILES_DSP = \
	PluguzuDSP.cpp


FILES_UI  = \
	PluguzuUI.cpp \
	dpf-widgets/opengl/DearImGui.cpp \
	dpf-widgets/opengl/DearImGuiColorTextEditor.cpp


# --------------------------------------------------------------
# Do some magic

include dpf/Makefile.plugins.mk

# Add the dear-imgui header path
BUILD_CXX_FLAGS += -I./dpf-widgets/opengl

# Add the haskell rts and stub header path
BUILD_CXX_FLAGS += -I./bin -I./bin/rts-header

# Add the foreign library and inject the special '$ORIGIN' rpath so that it can load the libpluguzu.so from the plugin folder.
LINK_FLAGS = -L./bin '-Wl,-rpath,$$ORIGIN' -lpluguzu

# vst3 puts the lib in a nested folder, so we need to patch to adjust the libpuguzu location
vst3-fix: vst3
	@ln -sf ../../../libpluguzu.so bin/pluguzu.vst3/Contents/x86_64-linux/


# --------------------------------------------------------------
# Haskell lib

haskell-lib:
	@cabal build
	@mkdir -p bin
	@find $(PWD)/dist-newstyle/ \( -name libpluguzu.so  -or -name Pluguzu_stub.h \) -exec ln -sf {} bin/ \;
	@ln -sf $(shell ghc-pkg field rts include-dirs --simple-output) bin/rts-header


# --------------------------------------------------------------
# Enable all possible plugin types

TARGETS += clap
TARGETS += vst2
TARGETS += vst3

all: haskell-lib $(TARGETS)


# --------------------------------------------------------------
# Install

install-vst2:
	@mkdir -p ~/.vst
	@ln -sf $(PWD)/bin/libpluguzu.so $(PWD)/bin/pluguzu-vst.so ~/.vst

install-vst3:
	@mkdir -p ~/.vst3
	@ln -sf $(PWD)/bin/pluguzu.vst3 ~/.vst3
	@ln -sf $(PWD)/bin/libpluguzu.so bin/pluguzu.vst3/Contents/x86_64-linux

install-clap:
	@mkdir -p ~/.clap
	@ln -sf $(PWD)/bin/libpluguzu.so $(PWD)/bin/pluguzu.clap ~/.clap

# backward compat
install-vst: install-vst2

install: install-vst2 install-vst3 install-clap

render-presets:
	@echo "renderPresetData" | cabal repl

# --------------------------------------------------------------
# Test helper

run-carla:
	carla ./demo.carxp

run-clap:
	/srv/github.com/free-audio/clap-host/builds/ninja-system/host/Debug/clap-host -p bin/pluguzu.clap

run-standalone:
	@g++ $(BUILD_CXX_FLAGS) $(LINK_FLAGS) -o bin/standalone standalone.cpp
	./bin/standalone

ghcid:
	ghcid -W --test PluguzuCore.standalone
