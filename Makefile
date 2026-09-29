default: build

build:
	stellar contract build

# The factory's tests deploy the real payroll wasm, so build it first.
wasm-payroll:
	cargo build -p disburs-payroll --target wasm32v1-none --release

test: wasm-payroll
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
	cargo build --locked -p disburs-payroll --target wasm32v1-none --release
	cargo test --locked --all-targets
	stellar contract build

.PHONY: default build wasm-payroll test fmt clippy clean ci
