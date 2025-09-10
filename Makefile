#!/usr/bin/make -f

all: haskell-lib build-rust

install:
	@echo "TODO"

render-presets:
	@echo "renderPresetData" | cabal repl
	@cargo fmt

build-rust:
	@cargo xtask bundle pluguzu

haskell-lib:
	@cabal build
	@mkdir -p bin
	@find $(PWD)/dist-newstyle/ \( -name libpluguzu.so  -or -name Pluguzu_stub.h \) -exec ln -sf {} bin/ \;
	@ln -sf $(shell ghc-pkg field rts include-dirs --simple-output) bin/rts-header

run-standalone:
	@cargo build
	@env LD_LIBRARY_PATH=./bin ./target/debug/pluguzu
