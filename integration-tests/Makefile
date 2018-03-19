.PHONY: all clean test

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS += --release
export TURTL_PATH := ../target/release/
export TURTL_CONFIG_FILE := ../config.yaml

all: build

build:
	$(CARGO) build $(CARGO_BUILD_ARGS)

test:
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

