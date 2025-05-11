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

BUILD_CXX_FLAGS += -I./dpf-widgets/opengl

# --------------------------------------------------------------
# Enable all possible plugin types


TARGETS += clap

all: $(TARGETS)

# --------------------------------------------------------------
