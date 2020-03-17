.PHONY: test

export PATH := $(PATH):../target/release
export LD_LIBRARY_PATH := $(LD_LIBRARY_PATH):../target/release
export TURTL_LOGLEVEL ?= off
export TURTL_CONFIG_FILE := ../config.yaml
CARGO := $(shell which cargo)

test:
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture
