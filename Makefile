.PHONY: all clean

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS :=

all: build

build: 
	cargo build

run: build
	cargo run

test:
	cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-st:
	RUST_TEST_TASKS=1 cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

doc:
	cargo doc -p turtl-core --no-deps

macros:
	cargo rustc -- -Z unstable-options --pretty=expanded

clean:
	rm -rf target/
	rm -f Cargo.lock

