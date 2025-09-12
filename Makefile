#!/usr/bin/make -f

all: haskell-lib build-rust

clean:
	@rm -Rf bin

install-vst3:
	@mkdir -p ~/.vst3
	@ln -sf $(PWD)/bin/pluguzu.vst3 ~/.vst3
	@ln -sf $(PWD)/bin/libpluguzu.so bin/pluguzu.vst3/Contents/x86_64-linux
	@chmod +x bin/pluguzu.vst3/Contents/x86_64-linux/pluguzu.so
	@echo "Installed VST3: ~/.vst3/pluguzu.vst3"

install-clap:
	@mkdir -p ~/.clap
	@ln -sf $(PWD)/bin/libpluguzu.so $(PWD)/bin/pluguzu.clap ~/.clap
	@chmod +x bin/pluguzu.clap
	@echo "Installed CLAP: ~/.clap/pluguzu.clap"

install: install-vst3 install-clap

render-presets:
	@echo "renderPresetData" | cabal repl
	@cargo fmt

build-rust:
	@cargo xtask bundle pluguzu
	@rm -Rf bin/pluguzu.vst3
	@mv target/bundled/pluguzu.clap target/bundled/pluguzu.vst3/ bin/

haskell-lib:
	@cabal build
	@mkdir -p bin
	@find $(PWD)/dist-newstyle/ \( -name libpluguzu.so  -or -name Pluguzu_stub.h \) -exec ln -sf {} bin/ \;
	@ln -sf $(shell ghc-pkg field rts include-dirs --simple-output) bin/rts-header

run-standalone:
	@cargo build
	@env LD_LIBRARY_PATH=./bin ./target/debug/pluguzu
