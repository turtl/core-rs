.PHONY: all clean release build test test-unit test-panic test-st doc macros

# non-versioned include
-include vars.mk

CARGO := $(shell which cargo)

all: build

build: 
	cargo build $(CARGO_BUILD_ARGS)

release: override CARGO_BUILD_ARGS += --release
release: build

test-unit:
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-int: override CARGO_BUILD_ARGS += --release
test-int:
	$(CARGO) test $(TEST) \
		-p integration-tests \
		$(CARGO_BUILD_ARGS) -- --nocapture

test: test-unit test-int

test-panic:
	RUST_BACKTRACE=1 \
		$(CARGO) test \
			--features "panic-on-error" \
			$(TEST) \
			$(CARGO_BUILD_ARGS) -- \
			--nocapture

test-st:
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture --test-threads 1

doc:
	$(CARGO) doc -p turtl_core --no-deps

macros:
	$(CARGO) rustc -- -Z unstable-options --pretty=expanded

clean:
	rm -rf target/
	cargo clean

