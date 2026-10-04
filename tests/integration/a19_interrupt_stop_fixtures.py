"""Small owned host fixtures for declared snapshot interruption points."""
from pathlib import Path

import a19_interrupt_stop_points as points

HOLD_READ = ("with (stage/'disk.raw').open('rb') as staged:\n"
             "    pathlib.Path(sys.argv[0] + '.held').touch()\n    time.sleep(30)\n")


def helper(path: Path, prelude: str, names: tuple[str, ...], before: str = "",
           hold: str = HOLD_READ) -> Path:
    # Never-qualified controls change selection before staged_ready can hold.
    path.write_text("#!/usr/bin/env python3\nimport os, pathlib, sys, time\n" + prelude + before +
                    "stage.mkdir(mode=0o700)\n"
                    f"for name in {names!r}:\n"
                    "    with (stage/name).open('wb') as output:\n"
                    "        output.write(b'staged'); output.flush(); os.fsync(output.fileno())\n"
                    + hold)
    path.chmod(0o700)
    return path


def fixture(root: Path, generation: bool = False) -> tuple[Path, Path, Path, Path, Path]:
    disk, vars, snapshot, output = (root / "disk.raw", root / "vars.fd", root / "snapshot", root / "output")
    disk.write_bytes(b"old-disk")
    vars.write_bytes(b"old-vars")
    snapshot.mkdir()
    output.mkdir()
    managed = points.stable_root(disk.resolve(), vars.resolve())
    if generation:
        (managed / "current").mkdir(parents=True, mode=0o700)
        for name in ("disk.raw", "vars.fd"):
            (managed / "current" / name).write_bytes(b"generation")
    return disk, vars, snapshot, output, managed
