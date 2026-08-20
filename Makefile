.PHONY: test api ingest fmt clippy

test:
	cargo test --workspace --lib --bins

test-db:
	cargo test --workspace

api:
	cargo run -p vi-api

ingest:
	cargo run -p vi-ingest

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
