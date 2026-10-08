#!/usr/bin/env python3
"""Both build invocations use unique output-local mount/provisioning roots."""
from pathlib import Path
import json
import sys

root = Path(sys.argv[1]).resolve()
commands = [json.loads(line) for line in (root / "hdiutil.log").read_text().splitlines()]
mounts = [Path(words[words.index("-mountpoint") + 1]) for words in commands if "-mountpoint" in words]
assert len(mounts) == 4
assert all(path.parent.parent == root / "output" and path.parent.name.startswith("bridgevm-win-source.") for path in mounts)
assert [path.name for path in mounts] == ["iso", "dst", "iso", "dst"]
assert mounts[0].parent == mounts[1].parent and mounts[2].parent == mounts[3].parent
assert mounts[0].parent != mounts[2].parent
assert (root / "output/source.raw").is_file()
assert not list(root.glob("bridgevm-win-source.*"))
print("PASS: WinPE scratch follows canonical output volume, not caller TMPDIR")
