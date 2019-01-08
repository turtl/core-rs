#!/bin/bash

set -e

VARS=vars.ios.mk
TMP=/tmp/turtl-ios

function cleanup() {
	echo "Cleaning up (reverting Cargo.toml and removing ${TMP}..."
	git checkout Cargo.toml
	rm -rf "${TMP}"
}
trap cleanup exit

mkdir -p "${TMP}"
cat "${VARS}" | grep -v '^export INCLUDE ' | sed 's/ := /=/g' > "${TMP}/vars.ios.sh"
source "${TMP}/vars.ios.sh"

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
		release

}

sed -i '' 's/crate-type = .*/crate-type = ["staticlib"]/g' Cargo.toml

build_arch armv7 armv7-apple-ios
build_arch armv7s armv7s-apple-ios
build_arch arm64 aarch64-apple-ios
build_arch x86_64 x86_64-apple-ios

mkdir -p target/ios
echo "Build finished, creating fat library in target/ios/libturtl_core.a"
lipo \
	-create \
	target/armv7-apple-ios/release/libturtl_core.a \
	target/armv7s-apple-ios/release/libturtl_core.a \
	target/aarch64-apple-ios/release/libturtl_core.a \
	target/x86_64-apple-ios/release/libturtl_core.a \
	-output target/ios/libturtl_core.a

