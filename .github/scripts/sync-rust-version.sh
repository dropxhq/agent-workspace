#!/usr/bin/env bash
# Sync [package] version in rust/Cargo.toml and agent-workspace entry in rust/Cargo.lock.
# Tag names like 2026.6.15a2 are normalized to Cargo semver 2026.6.15-a2.
set -euo pipefail

TAG="${1:?usage: sync-rust-version.sh <version>}"
export TAG

CARGO_VERSION="$(python3 - <<'PY'
import os
import pathlib
import re

tag = os.environ["TAG"]
match = re.fullmatch(r"(\d{4}\.\d{1,2}\.\d{1,2})([a-z][0-9A-Za-z.-]*)", tag)
cargo_version = f"{match.group(1)}-{match.group(2)}" if match else tag

toml = pathlib.Path("rust/Cargo.toml")
text = toml.read_text(encoding="utf-8")
text = re.sub(
    r'(\[package\]\s*\n(?:[^\[]*\n)*?^version = )"[^"]+"',
    rf'\1"{cargo_version}"',
    text,
    count=1,
    flags=re.M,
)
toml.write_text(text, encoding="utf-8")

lock = pathlib.Path("rust/Cargo.lock")
lock_text = lock.read_text(encoding="utf-8")
lock_text = re.sub(
    r'(name = "agent-workspace"\s*\nversion = )"[^"]+"',
    rf'\1"{cargo_version}"',
    lock_text,
    count=1,
)
lock.write_text(lock_text, encoding="utf-8")

print(cargo_version)
PY
)"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "cargo_version=$CARGO_VERSION" >> "$GITHUB_OUTPUT"
fi
