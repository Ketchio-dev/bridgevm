"""Fixed T12 input identities; final guest-written vars are outputs, not seals."""
from contextlib import ExitStack
import hashlib
from pathlib import Path
import re

from d11_fixture_files import FileSeal, read

TIER = "t12-bridgevm-pc-nvme-block"
SHA = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")
IMAGE = hashlib.sha256(b"BRIDGEVM" + bytes(1048576 - 8)).hexdigest()
VARS = hashlib.sha256(bytes([255]) * 65536).hexdigest()
FIXED = {"schema": "bridgevm.t12-sealed-inputs.v1", "binary_profile": "debug",
         "binary_features": "default", "image_recipe": "bridgevm-pc-nvme-marker-v1",
         "image_sha256": IMAGE, "initial_vars_recipe": "bridgevm-pc-erased-vars-v1",
         "initial_vars_sha256": VARS}
KEYS = set(FIXED) | {"binary_source_commit", "rust_toolchain", "binary", "firmware"}


def parse(data, commit, repo):
    rows = {}
    for line in data.decode("utf-8").splitlines():
        fields = line.split("\t")
        key, value = fields[0], fields[1:]
        if key not in KEYS or key in rows or len(value) != (2 if key in ("binary", "firmware") else 1):
            raise ValueError("T12 manifest fields differ")
        rows[key] = value
    if set(rows) != KEYS or not COMMIT.fullmatch(commit):
        raise ValueError("T12 manifest/source identity missing")
    if rows["binary_source_commit"] != [commit] or any(rows[k] != [v] for k, v in FIXED.items()):
        raise ValueError("T12 source/profile/generated input differs")
    if not re.fullmatch(r"[1-9][0-9]*\.[0-9]+\.[0-9]+", rows["rust_toolchain"][0]):
        raise ValueError("T12 Rust toolchain must be explicit")
    for key in ("binary", "firmware"):
        if not rows[key][0].startswith("/") or not SHA.fullmatch(rows[key][1]):
            raise ValueError("T12 artifact identity malformed")
    pin = read(repo / "crates/bridgevm-hvf/firmware/bridgevm-pc-dxe-entry.sha256", 65).decode("ascii")
    if len(pin) != 65 or pin[-1] != "\n" or not SHA.fullmatch(pin[:-1]) or rows["firmware"][1] != pin[:-1]:
        raise ValueError("T12 firmware differs from the public pin")
    return rows


class Inputs:
    def __init__(self, path, commit, repo, copied_binary=None):
        self.stack = ExitStack()
        try:
            self.data = read(path, 16384)
            self.rows = parse(self.data, commit, repo)
            self.binary = self.stack.enter_context(FileSeal(Path(self.rows["binary"][0]), 128 << 20, self.rows["binary"][1]))
            if not self.binary.identity[2] & 0o111:
                raise ValueError("T12 runner is not executable")
            self.firmware = self.stack.enter_context(FileSeal(Path(self.rows["firmware"][0]), 64 << 20, self.rows["firmware"][1]))
            if copied_binary is not None:
                self.stack.enter_context(FileSeal(copied_binary, 128 << 20, self.rows["binary"][1]))
        except BaseException:
            self.stack.close()
            raise

    def __enter__(self): return self
    def __exit__(self, *args): self.stack.close()


def binding(data, rows):
    return {"schema": "bridgevm.t12-submitted-inputs.v1",
            "input_manifest_sha256": hashlib.sha256(data).hexdigest(),
            "binary_sha256": rows["binary"][1], "firmware_sha256": rows["firmware"][1],
            **{key: rows[key][0] for key in KEYS - {"binary", "firmware", "schema"}}}
