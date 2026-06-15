#!/usr/bin/env python3
"""Minimal hook example: identity transform (stdin → stdout unchanged).

Useful for verifying hook wiring, timeouts, and ``WS_HOOK`` / ``WS_PATH`` env vars
without changing content. Configure both read and write to this script for a
no-op round trip.
"""
import sys


def main() -> None:
    sys.stdout.write(sys.stdin.read())


if __name__ == "__main__":
    main()
