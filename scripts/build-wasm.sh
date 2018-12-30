#!/bin/bash

make \
	FEATURES="sqlite-static wasm" \
	CARGO_BUILD_ARGS="--target wasm32-unknown-unknown" \
	release

