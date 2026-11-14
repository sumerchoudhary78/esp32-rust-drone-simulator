.PHONY: run test check browser native
run:
	cargo run --release
test:
	cargo test --workspace
check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
browser:
	npm run test:browser
native:
	cargo run --manifest-path apps/controller/Cargo.toml
