.PHONY: all clean release build test test-panic test-st doc macros

# non-versioned include
VARS ?= vars.mk
-include $(VARS)

CARGO ?= $(shell which cargo)
FEATURES ?= sqlite-static
override CARGO_BUILD_ARGS += --features "$(FEATURES)"

all: build

build: 
	$(CARGO) build $(CARGO_BUILD_ARGS)

release: override CARGO_BUILD_ARGS += --release
release: build

test:
	$(CARGO) test $(TEST) $(CARGO_BUILD_ARGS) -- --nocapture

test-panic: override FEATURES += panic-on-error
test-panic:
	RUST_BACKTRACE=1 \
		$(CARGO) test \
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

