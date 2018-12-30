#!/bin/bash


function cleanup() {
	git checkout Cargo.toml
}
trap cleanup exit

sed -i '' 's/crate-type = .*/crate-type = ["staticlib"]/g' Cargo.toml
make \
	BUILDCMD=lipo \
	VARS=vars.ios.mk \
	release

