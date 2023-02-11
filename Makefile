.PHONY: all install-deps deps build release lint fmt test unittest-coverage run clean pre-commit

RUST_EDITION ?= 2021

all: build

install-deps:
	rustup component add rustfmt
	rustup component add clippy
	rustup component add llvm-tools-preview
	cargo install cargo-udeps
	cargo install cargo-audit

deps:
	cargo update

build:
	 cargo build --verbose

release:
	 cargo build --release

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt --all -- --check

test: lint fmt
	cargo test

unittest-coverage:
	rm -Rf target/coverage
	mkdir -p target/coverage/profiles
	CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='target/coverage/profiles/cargo-test-%p-%m.profraw' cargo test
	grcov ./target/coverage/profiles/ --binary-path ./target/debug/deps/ -s . -t markdown --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o coverage.md

run: test
	RUST_LOG=debug cargo run --bin server

clean:
	cargo clean

pre-commit:
	pre-commit install
	pre-commit autoupdate
	pre-commit gc
