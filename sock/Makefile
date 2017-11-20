.PHONY: all clean

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS ?=

all: build

release: CARGO_BUILD_ARGS := $(CARGO_BUILD_ARGS) --release
release: all

run:
	cargo run $(CARGO_BUILD_ARGS)

build: 
	cargo build $(CARGO_BUILD_ARGS)

test:
	TURTL_LOGLEVEL=$(TEST_LOGLEVEL) cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-panic:
	RUST_BACKTRACE=1 \
	TURTL_LOGLEVEL=$(TEST_LOGLEVEL) \
		cargo test \
			--features "panic-on-error" \
			$(TEST) \
			$(CARGO_BUILD_ARGS) -- \
			--nocapture

test-st:
	TURTL_LOGLEVEL=$(TEST_LOGLEVEL) cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture --test-threads 1

doc:
	cargo doc -p turtl-core --no-deps

macros:
	cargo rustc -- -Z unstable-options --pretty=expanded

clean:
	rm -rf target/

