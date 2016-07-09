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

doc:
	cargo doc -p turtl-core --no-deps

clean:
	rm -rf target/

