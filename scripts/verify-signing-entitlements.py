#!/usr/bin/env python3
"""Validate exact Boolean entitlement values in bounded codesign plist output."""
import argparse
import plistlib
import sys
from xml.parsers.expat import ExpatError


class UniqueKeys(dict):
    def __setitem__(self, key, value):
        if key in self:
            raise ValueError(f"duplicate entitlement key: {key}")
        super().__setitem__(key, value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("required_key")
    parser.add_argument("--profile", choices=("debug", "release", "0", "1"), required=True)
    args = parser.parse_args()
    try:
        raw = sys.stdin.buffer.read(65537)
        if len(raw) > 65536:
            raise ValueError("entitlement plist exceeds 64 KiB")
        values = plistlib.loads(raw, dict_type=UniqueKeys)
        if not isinstance(values, dict) or values.get(args.required_key) is not True:
            raise ValueError(f"{args.required_key} must be Boolean true")
        debug_key = "com.apple.security.get-task-allow"
        if debug_key in values and type(values[debug_key]) is not bool:
            raise ValueError(f"{debug_key} must be Boolean")
        if args.profile in ("release", "1") and values.get(debug_key) is True:
            raise ValueError("release executable retains debug get-task-allow")
    except (ValueError, TypeError, plistlib.InvalidFileException, ExpatError) as error:
        print(f"entitlement verification failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
