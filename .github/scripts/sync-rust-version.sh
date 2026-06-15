#!/usr/bin/env bash
# Sync [package] version in rust/Cargo.toml and agent-workspace entry in rust/Cargo.lock.
set -euo pipefail

TAG="${1:?usage: sync-rust-version.sh <version>}"
export TAG

python3 - <<'PY'
import os
import pathlib
import re

tag = os.environ["TAG"]
toml = pathlib.Path("rust/Cargo.toml")
text = toml.read_text()
text = re.sub(
    r'(\[package\]\s*\n(?:[^\[]*\n)*?^version = )"[^"]+"',
    rf'\1"{tag}"',
    text,
    count=1,
    flags=re.M,
)
toml.write_text(text)

lock = pathlib.Path("rust/Cargo.lock")
lock_text = lock.read_text()
lock_text = re.sub(
    r'(name = "agent-workspace"\s*\nversion = )"[^"]+"',
    rf'\1"{tag}"',
    lock_text,
    count=1,
)
lock.write_text(lock_text)
PY
