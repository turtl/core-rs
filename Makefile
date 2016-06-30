.PHONY: all clean

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)

all:
	cargo build

run: all
	cargo run

clean:
	rm -rf target/

