RUST_EDITION ?= 2021

.PHONY: all
all: build

.PHONY: venv
venv:
	python3 -m venv venv

.PHONY: install-deps
install-deps: venv
	rustup component add rustfmt
	rustup component add clippy
	rustup component add llvm-tools-preview
	cargo install cargo-udeps
	cargo install cargo-audit
	. ./venv/bin/activate && pip install -r requirements.txt

.PHONY: deps
deps:
	cargo update

.PHONY: fmt
fmt: install-deps
	cargo fmt --all

.PHONY: lint
lint: install-deps
	cargo clippy --fix --allow-dirty --allow-staged

.PHONY: build
build:
	 cargo build --release

.PHONY: test
test: unittest
	cargo fmt --all -- --check
	cargo clippy -- -D warnings

.PHONY: unittest
unittest: install-deps
	cargo test

.PHONY: unittest-coverage
unittest-coverage: install-deps
	rm -Rf target/coverage
	mkdir -p target/coverage/profiles
	CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='target/coverage/profiles/cargo-test-%p-%m.profraw' cargo test
	grcov ./target/coverage/profiles/ --binary-path ./target/debug/deps/ -s . -t markdown --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o coverage.md


.PHONY: run
run: test
	RUST_LOG=debug cargo run --bin server

.PHONY: clean
clean:
	cargo clean

.PHONY: pre-commit
pre-commit:
	pre-commit install
	pre-commit autoupdate
	pre-commit gc
