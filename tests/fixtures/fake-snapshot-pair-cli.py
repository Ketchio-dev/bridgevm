#!/usr/bin/env python3
"""Fixture snapshot_pair_cli: `digest <disk> <vars>` for lanes without a managed generation."""
import hashlib, sys

if len(sys.argv) != 4 or sys.argv[1] != "digest":
    sys.exit(2)
for name, path in (("disk", sys.argv[2]), ("vars", sys.argv[3])):
    data = open(path, "rb").read()
    print(f"{name}_bytes {len(data)}\n{name}_sha256 {hashlib.sha256(data).hexdigest()}")
