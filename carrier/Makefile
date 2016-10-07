.PHONY: all build clean crun test test-st run

rustfiles := $(shell find src -name "*.rs")

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)
CARGO_BUILD_ARGS :=

all: build

build: 
	cargo build

run: build
	cargo run

test:
	cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-st:
	RUST_TEST_TASKS=1 cargo test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

target/debug/carrier.lib: $(rustfiles)
	rm -f $@
	cargo build

target/test.exe: test.c target/debug/carrier.lib
	gcc \
		-o $@ \
		test.c \
		-L/usr/lib/ \
		-Ltarget/debug/ \
		-lcarrier \
		-lsqlite3 \
		-lkernel32 \
		-luserenv \
		-lshell32 \
		-ladvapi32 \
		-lgcc_eh \
		-lws2_32 \
		-lwsock32

crun: target/test.exe
	./target/test

clean:
	rm -f target/test.exe target/debug/carrier.lib
