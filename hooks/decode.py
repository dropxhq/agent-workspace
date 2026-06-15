#!/usr/bin/env python3
"""Read hook example: convert physical storage to logical content.

Physical files are stored with an ``ENC:`` prefix; this script strips it on read.
See ``encode.py`` for the matching write hook.
"""
import sys


def main() -> None:
    data = sys.stdin.read()
    if data.startswith("ENC:"):
        data = data[4:]
    sys.stdout.write(data)


if __name__ == "__main__":
    main()
