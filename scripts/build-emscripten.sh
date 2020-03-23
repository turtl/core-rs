#!/bin/bash

source /home/andrew/src/emsdk/emsdk_env.sh
make \
	FEATURES="sqlite-static wasm" \
	CARGO_BUILD_ARGS="--target wasm32-unknown-emscripten" \
	PKG_CONFIG_ALLOW_CROSS=1 \
	RUSTFLAGS="-L/usr/include/x86_64-linux-gnu -L/usr/include" \
	release

