#!/usr/bin/env python3
"""Temporary host mountpoints are private to each build and cleaned on failure."""
from pathlib import Path
import json
import subprocess
import sys

root = Path(sys.argv[1]).resolve()
commands = [json.loads(line) for line in (root / "hdiutil.log").read_text().splitlines()]
mounts = [Path(words[words.index("-mountpoint") + 1]) for words in commands if "-mountpoint" in words]
assert len(mounts) == 4
host_temp = Path(subprocess.check_output(["getconf", "DARWIN_USER_TEMP_DIR"], text=True).strip()).resolve()
failed = [json.loads(line) for line in (root / "failed.log").read_text().splitlines()]
assert len(failed) == 1 and "-readonly" in failed[0]
failed_mount = Path(failed[0][failed[0].index("-mountpoint") + 1])
for mount in [*mounts, failed_mount]:
    assert mount.parent.parent.resolve() == host_temp
    assert not mount.parent.exists(), mount.parent
assert [path.name for path in mounts] == ["iso", "dst", "iso", "dst"]
assert mounts[0].parent == mounts[1].parent and mounts[2].parent == mounts[3].parent
assert len({mounts[0].parent, mounts[2].parent, failed_mount.parent}) == 3
assert (root / "output/source.raw").is_file()
assert not list((root / "output").glob("bridgevm-win-source.*"))
print("PASS: WinPE staging stays on the output volume and host mountpoints are cleaned")
