#!/usr/bin/env python3
"""Execute injection staging on immutable fixtures; no Windows behaviour claim."""
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
helper = (ROOT / "scripts/live-gates/windows-closure-stage.sh").read_text()


def check(script: str) -> bool:
    with tempfile.TemporaryDirectory(prefix="closure stage ") as tmp:
        root = Path(tmp)
        stage = root / "stage"
        stage.mkdir()
        env = dict(os.environ, STAGE=str(stage))
        sources = []
        for key, name in (("IMAGE", "disk.raw"), ("INJECTOR_VARS", "vars.fd"),
                          ("INJECTOR", "injector.raw")):
            source = root / name
            source.write_bytes(key.encode())
            source.chmod(0o400)
            env[key] = str(source)
            sources.append((source, stage / name, key.encode()))
        result = subprocess.run(["bash", "-c", script + "\nstage_closure_inputs"],
                                env=env, capture_output=True)
        if result.returncode:
            return False
        for source, clone, data in sources:
            if not clone.exists() or source.stat().st_ino == clone.stat().st_ino:
                return False
            if source.stat().st_mode & 0o777 != 0o400 or clone.stat().st_mode & 0o777 != 0o600:
                return False
            if source.read_bytes() != data or clone.read_bytes() != data:
                return False
            with clone.open("r+b") as stream:
                stream.write(b"changed")
            if source.read_bytes() != data:
                return False
        return True


assert check(helper), "immutable inputs must produce independent writable staging files"
chmod = 'chmod 600 "$STAGE/disk.raw" "$STAGE/vars.fd" "$STAGE/injector.raw"'
assert chmod in helper
assert not check(helper.replace(chmod, "true")), "missing chmod accepted"
assert not check(helper.replace(chmod, 'chmod 600 "$IMAGE" "$INJECTOR_VARS" "$INJECTOR"')), "source chmod accepted"
for name in ("disk.raw", "vars.fd", "injector.raw"):
    assert not check(helper.replace(chmod, chmod.replace(f' "$STAGE/{name}"', ""))), name
print("PASS: immutable injection clones and five permission rejection mutations")
