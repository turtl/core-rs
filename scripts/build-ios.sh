#!/bin/bash

VARS=vars.ios.mk
TMP=/tmp/turtl-ios

source "${VARS}"

function cleanup() {
	git checkout Cargo.toml
	rm -rf "${TMP}"
}
trap cleanup exit

# creates a temporary directory for all our native libs and extracts the
# underlying static lib from the fat lipo archive into that tmp dir
function build_arch() {
	ios_target="$1"
	rust_target="$2"

	mkdir -p "${TMP}/target-${ios_target}"

	# for each native library, extract into the target-specific dir
	lipo "${SODIUM_LIB_DIR}/libsodium.a" -thin ${ios_target} -output "${TMP}/target-${ios_target}/libsodium.a"
	lipo "${OPENSSL_LIB_DIR}/libcrypto.a" -thin ${ios_target} -output "${TMP}/target-${ios_target}/libcrypto.a"
	lipo "${OPENSSL_LIB_DIR}/libssl.a" -thin ${ios_target} -output "${TMP}/target-${ios_target}/libssl.a"
	make \
		VARS=vars.ios.mk \
		SODIUM_LIB_DIR="${TMP}/target-${ios_target}" \
		OPENSSL_LIB_DIR="${TMP}/target-${ios_target}" \
		CARGO_BUILD_ARGS="--target ${rust_target}" \
		FEATURES="sqlite-static ios" \
		release

}

build_arch armv7 armv7-apple-ios
build_arch armv7s armv7s-apple-ios
build_arch arm64 aarch64-apple-ios
build_arch x86_64 x86_64-apple-ios

sed -i '' 's/crate-type = .*/crate-type = ["staticlib"]/g' Cargo.toml
