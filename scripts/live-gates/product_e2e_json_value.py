#!/usr/bin/env python3
"""Print one scalar JSON value by dotted path for the T17 and T19 tier shells; booleans print lowercase."""
import json, sys
value = json.load(open(sys.argv[1], encoding="utf-8"))
for part in sys.argv[2].split("."):
    value = value[part]
if not isinstance(value, (str, int, bool)):
    raise SystemExit(2)
print(str(value).lower() if isinstance(value, bool) else value)
