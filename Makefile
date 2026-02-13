
.PHONY: all build install clean

all: install

build:
	cargo build --release

install: build
	mkdir -p ~/.local/bin
	cp target/release/gofi ~/.local/bin/gofi

clean:
	cargo clean
