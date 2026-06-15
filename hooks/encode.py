#!/usr/bin/env python3
"""Write hook example: convert logical content to physical storage.

Adds an ``ENC:`` prefix before persisting. Pair with ``decode.py`` on read.
"""
import sys


def main() -> None:
    data = sys.stdin.read()
    sys.stdout.write("ENC:" + data)


if __name__ == "__main__":
    main()
