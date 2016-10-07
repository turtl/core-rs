.PHONY: crun

rustfiles := $(shell find src -name "*.rs")

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
