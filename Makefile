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
	@find $(PWD)/dist-newstyle/ -name libpluguzu.so  -exec ln -sf {} bin/ \;
	@find $(PWD)/dist-newstyle/ -name Pluguzu_stub.h -exec ln -sf {} bin/ \;
	@find ~/.ghcup/ -name "rts-*" -type d -exec ln -sf {}/include bin/rts-header \;


# --------------------------------------------------------------
# Enable all possible plugin types

TARGETS += clap
TARGETS += vst2
TARGETS += vst3

all: haskell-lib vst3-fix $(TARGETS)

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
