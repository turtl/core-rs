.PHONY: all clean

# non-versioned include
-include ../vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS :=

all: build

build: 
	cargo build

run: build
	cargo run

test:
	cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture --test-threads 1

doc:
	cargo doc -p turtl-core --no-deps

macros:
	cargo rustc -- -Z unstable-options --pretty=expanded

clean:
	rm -rf target/
	rm -f Cargo.lock

