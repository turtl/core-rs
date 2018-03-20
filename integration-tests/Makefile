.PHONY: test

export PATH := $(PATH):../target/release
export LD_LIBRARY_PATH := $(shell realpath ../target/release):$(LD_LIBRARY_PATH)
export TURTL_LOGLEVEL ?= off
export TURTL_CONFIG_FILE := ../config.yaml
CARGO := $(shell which cargo)

test:
	@echo "Paths $(LD_LIBRARY_PATH)"
	ls -al $(shell realpath ../target/release)
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture
