default: build

build:
	stellar contract build

# The tests deploy and upgrade the real wasm, so build both first.
wasm:
	cargo build -p disburs-payroll -p disburs-factory --target wasm32v1-none --release

test: wasm
	cargo test

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets -- -D warnings

clean:
	cargo clean

# Everything CI enforces, in the order CI runs it.
ci:
	cargo fmt --all --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo build --locked -p disburs-payroll -p disburs-factory --target wasm32v1-none --release
	cargo test --locked --all-targets
	stellar contract build

.PHONY: default build wasm test fmt clippy clean ci
