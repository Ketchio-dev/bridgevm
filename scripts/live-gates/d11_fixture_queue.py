#!/usr/bin/env python3
"""D11 queue entry: a development fixture, never a release/live criterion."""
import json
import os
from pathlib import Path
import sys
import subprocess

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from d11_fixture_files import record
from d11_fixture_inputs import Inputs
from d11_fixture_queue_inputs import bound, create_output, validate, seal
from d11_fixture_receipt import empty, collect, finalize, publish, guard, archived
from d11_fixture_runtime import execute
from t22_pair_queue import require_source


def run(directory, root, commit, manifest, binary):
    if manifest != directory / "input-manifest.tsv" or binary != directory / "hvf_gic_boot_probe":
        raise ValueError("fixture inputs are not queue-owned")
    binding, _ = bound(directory, commit)
    try:
        with Inputs(manifest, commit, binary) as inputs:
            output = create_output(binding["job_id"])
            execute(directory, root, inputs, output, binary)
            inputs.check()
        require_source(root, commit)
        value = collect(directory, commit)
    except (OSError, ValueError, subprocess.SubprocessError):
        record(directory / "d11-refusal.private.json", {"reason": "admission-or-integrity-refused"})
        value = empty(binding, "cleanup-unproved")
    record(directory / "receipt.json", value)
    return 0 if value["sealed_fixture"] else 1


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
