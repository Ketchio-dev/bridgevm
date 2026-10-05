#!/usr/bin/env python3
"""D11 queue entry: a development fixture, never a release/live criterion."""
import json
import os
from pathlib import Path
import sys
import subprocess

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from d11_fixture_queue_inputs import validate, seal
from d11_fixture_receipt import finalize, publish, guard, archived
from d11_fixture_attempt import run
from t22_pair_queue import require_source


def main():
    os.umask(0o077)
    mode, *args = sys.argv[1:]
    if mode in ("validate", "seal"):
        path, commit, *rest = args
        if mode == "validate": validate(Path(path), commit)
        else: seal(Path(path), commit, Path(rest[0]))
        return 0
    directory, root, commit, *rest = args
    directory, root = Path(directory), Path(root)
    require_source(root, commit)
    if mode == "run": return run(directory, root, commit, Path(rest[0]), Path(rest[1]))
    if mode == "finalize": finalize(directory, commit)
    elif mode == "publish": publish(directory, commit)
    elif mode == "guard": guard(directory, commit, rest[0])
    elif mode == "archive": print(json.dumps(archived(directory, commit, rest[0]), sort_keys=True))
    else: raise ValueError("unknown fixture operation")
    return 0


if __name__ == "__main__":
    try: raise SystemExit(main())
    except (OSError, ValueError, IndexError, subprocess.SubprocessError) as error:
        raise SystemExit("D11 operation refused: " + type(error).__name__) from None
