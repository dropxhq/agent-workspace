# Agent Workspace — build & package helpers
# Run `just` or `just --list` to see available recipes.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

rust_dir := "rust"
python_dir := "python"
rust_manifest := rust_dir + "/Cargo.toml"
rust_release_bin := rust_dir + "/target/release/ws"
rust_wheels_dir := rust_dir + "/target/wheels"

default:
    @just --list

# --- Rust ---

# Release-build the `ws` CLI binary.
pack-rust:
    cargo build --release --manifest-path {{rust_manifest}} --locked
    @echo "Binary: {{rust_release_bin}}"

# Alias for local development (debug build).
build-rust:
    cargo build --manifest-path {{rust_manifest}} --locked
    @echo "Binary: {{rust_dir}}/target/debug/ws"

test-rust:
    cargo test --manifest-path {{rust_manifest}} --locked

clippy-rust:
    cargo clippy --all-targets --manifest-path {{rust_manifest}} --locked

# --- Python ---

# Build release wheel(s) via maturin (abi3).
pack-python:
    cd {{python_dir}} && uv sync --locked && uv run maturin build --release --locked
    @echo "Wheels: {{rust_wheels_dir}}/"

# Install editable package + native extension into python/.venv (dev loop).
develop-python:
    cd {{python_dir}} && uv sync --locked && uv run maturin develop --locked

sync-python:
    cd {{python_dir}} && uv sync --locked

# --- Combined ---

pack: pack-rust pack-python
