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
test: install-deps
	cargo fmt --all -- --check
	cargo clippy -- -D warnings
	cargo test

.PHONY: run
run: test
	RUST_LOG=debug cargo run --bin converter

.PHONY: clean
clean:
	cargo clean

.PHONY: pre-commit
pre-commit:
	pre-commit install
	pre-commit autoupdate
	pre-commit gc

.PHONY: python-generate-protos
python-generate-protos:
	$(MAKE) -C client generate-protos
