.PHONY: build test clean

UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

ifeq ($(UNAME_S),Darwin)
	PLATFORM := darwin
else ifeq ($(UNAME_S),Linux)
	PLATFORM := linux
else
	$(error Unsupported platform: $(UNAME_S))
endif

ifeq ($(UNAME_M),x86_64)
	ARCH := amd64
else ifeq ($(UNAME_M),arm64)
	ARCH := arm64
else
	$(error Unsupported architecture: $(UNAME_M))
endif

DIST_DIR := dist/$(PLATFORM)-$(ARCH)
LIB_TARGET := $(DIST_DIR)/libmonty_ffi.a

build: include/monty_ffi.h $(LIB_TARGET)

include/monty_ffi.h: monty-ffi/src/lib.rs monty-ffi/cbindgen.toml
	cd monty-ffi && cbindgen --config cbindgen.toml --output ../include/monty_ffi.h

$(LIB_TARGET): monty-ffi/src/lib.rs monty-ffi/Cargo.toml
	cd monty-ffi && cargo build --release
	mkdir -p $(DIST_DIR)
	cp monty-ffi/target/release/libmonty_ffi.a $(LIB_TARGET)

test: build
	go test ./pkg/monty/...

clean:
	rm -f include/monty_ffi.h
	rm -rf dist
	cd monty-ffi && cargo clean
