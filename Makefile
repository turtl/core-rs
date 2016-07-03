.PHONY: all clean

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS :=

all:
	cargo build $(CARGO_BUILD_ARGS)

run: all
	cargo run

test:
	cargo test $(CARGO_BUILD_ARGS)

clean:
	rm -rf target/

