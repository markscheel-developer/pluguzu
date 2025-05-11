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

BUILD_CXX_FLAGS += -I./dpf-widgets/opengl -I./dist-newstyle/build/x86_64-linux/ghc-9.12.2/pluguzu-0.1/f/pluguzu/build/pluguzu/pluguzu-tmp/ -I/home/fedora/.ghcup/ghc/9.12.2/lib/ghc-9.12.2/lib/x86_64-linux-ghc-9.12.2-5a14/rts-1.0.2/include/
LINK_FLAGS = -L./dist-newstyle/build/x86_64-linux/ghc-9.12.2/pluguzu-0.1/f/pluguzu/build/pluguzu/ -lpluguzu

# --------------------------------------------------------------
# Enable all possible plugin types


TARGETS += clap

all: $(TARGETS)

# --------------------------------------------------------------

run:
	env LD_LIBRARY_PATH=./dist-newstyle/build/x86_64-linux/ghc-9.12.2/pluguzu-0.1/f/pluguzu/build/pluguzu /srv/github.com/free-audio/clap-host/builds/ninja-system/host/Debug/clap-host -p bin/pluguzu.clap
