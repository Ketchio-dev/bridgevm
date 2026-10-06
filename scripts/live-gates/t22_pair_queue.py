#!/usr/bin/env python3
"""D10 development-only physical queue adapter; never a release criterion."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys

from t22_pair_environment import operations_environment, controlled_env
from t22_pair_queue_inputs import validate, seal, job, bound, stable_output
from t22_pair_queue_command import launch, command, store
from t22_pair_queue_receipt import empty, collect, finalize, publish, guard
from t22_pair_publication import write_exclusive


def require_source(root, commit):
    git = ["/usr/bin/git", "--no-optional-locks", "-c", "core.fsmonitor=false", "-c", "core.untrackedCache=false"]
    if (subprocess.check_output([*git, "rev-parse", "HEAD"], cwd=root, text=True, env=controlled_env(), timeout=30).strip() != commit
            or subprocess.check_output([*git, "status", "--porcelain", "--untracked-files=all"], cwd=root, env=controlled_env(), timeout=30)):
        raise ValueError("development source differs from sealed checkout")


def run(directory, root, commit, source, binary):
    identity = job(directory, commit)
    if source != directory / "input-manifest.tsv" or binary != directory / "hvf_gic_boot_probe":
        raise ValueError("development inputs are not queue-owned")
    require_source(root, commit)
    identity, rows, native, docs = bound(directory, root, commit)
    candidate = directory / "native-candidate.tsv"
    write_exclusive(candidate, native); candidate.chmod(0o400)
    parent = stable_output(identity["job_id"]).parent
    parent.mkdir(mode=0o700, parents=False, exist_ok=True)
    info = parent.lstat()
    if (parent.is_symlink() or parent.resolve() != parent or info.st_uid != os.geteuid()
            or info.st_dev != Path.home().stat().st_dev or info.st_mode & 0o077):
        raise ValueError("stable preparation parent is unsafe")
    try:
        launch(command(directory, root, identity, hashlib.sha256(native).hexdigest()), directory, identity)
        require_source(root, commit)
        value = collect(directory, root, commit)
    except (OSError, ValueError, subprocess.SubprocessError):
        value = empty(identity, "incomplete")
    store(directory / "receipt.json", value)
    return 0 if value["preparation_complete"] else 1


def main():
    mode, *args = sys.argv[1:]
    root = Path(__file__).resolve().parents[2]
    with operations_environment():
        if mode in ("validate", "seal"):
            path, commit, *rest = args
            if mode == "validate": validate(Path(path), commit, root)
            else: seal(Path(path), commit, Path(rest[0]), root)
            return 0
        if mode == "run":
            directory, root_arg, commit, source, binary = args
            return run(Path(directory), Path(root_arg), commit, Path(source), Path(binary))
        directory, root_arg, commit, *rest = args
        root = Path(root_arg); require_source(root, commit)
        if mode == "guard": guard(Path(directory), root, commit, rest[0])
        elif mode == "finalize": finalize(Path(directory), root, commit)
        elif mode == "publish": publish(Path(directory), root, commit)
        else: raise ValueError("unknown development queue operation")
    return 0


if __name__ == "__main__":
    try: raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit("D10 operation refused: " + type(error).__name__) from None
