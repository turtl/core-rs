.PHONY: all clean

# non-versioned include
-include ../vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS :=

all: build

build: 
	cargo build

test:
	TURTL_LOGLEVEL=$(TEST_LOGLEVEL) cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-st:
	TURTL_LOGLEVEL=$(TEST_LOGLEVEL) cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture --test-threads 1
