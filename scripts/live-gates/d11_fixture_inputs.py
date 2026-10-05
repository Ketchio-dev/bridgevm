"""Exact D11 development inputs, independent of installed fixture credentials."""
from contextlib import ExitStack
from pathlib import Path
import re

from d11_fixture_files import FileSeal, TreeSeal, canonical, read

TIER = "d11-native-fixture-preparation"
SCHEMA = "bridgevm.d11-fixture-input.v1"
SHA = re.compile(r"[0-9a-f]{64}\Z")
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,95}\Z")
FILES = {"iso", "payload_manifest", "binary", "renderer", "firmware"}
TREES = {"payload", "tools", "fixture_helper"}
META = {"schema", "classification", "source_commit", "binary_source_commit", "binary_profile",
        "binary_features", "rust_toolchain", "container_gib"}
FIRMWARE = "b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
RESOURCES = "D11Resources.bundle/Contents/Resources"
SEED = "windows-boot-seed-vars.fd.gz"
POLICY = "secureboot-microsoft-windows-transition-aarch64-v1.6.5.json"


def parse(data, commit):
    if not 0 < len(data) <= 65536 or b"\0" in data or not COMMIT.fullmatch(commit):
        raise ValueError("invalid fixture envelope")
    rows = {}
    for line in data.decode("utf-8").splitlines():
        key, *fields = line.split("\t")
        if key in rows or key not in FILES | TREES | META:
            raise ValueError("unknown or duplicate fixture field")
        if len(fields) != (2 if key in FILES | TREES else 1):
            raise ValueError("fixture field count differs")
        if key in FILES | TREES:
            canonical(fields[0])
            if not SHA.fullmatch(fields[1]): raise ValueError("invalid fixture digest")
        rows[key] = fields
    if set(rows) != FILES | TREES | META: raise ValueError("incomplete fixture envelope")
    fixed = {"schema": SCHEMA, "classification": "DEVELOPMENT_ONLY", "source_commit": commit,
             "binary_profile": "release", "binary_features": "venus"}
    if any(rows[k] != [v] for k, v in fixed.items()): raise ValueError("fixture policy differs")
    if (not COMMIT.fullmatch(rows["binary_source_commit"][0])
            or not re.fullmatch(r"1\.[0-9]{1,3}\.[0-9]{1,2}", rows["rust_toolchain"][0])
            or rows["container_gib"][0] not in {str(n) for n in range(16, 33)}
            or rows["firmware"][1] != FIRMWARE):
        raise ValueError("fixture build or storage policy differs")
    if len({rows[k][0] for k in FILES | TREES}) != len(FILES | TREES):
        raise ValueError("fixture roles alias")
    return rows


class Inputs:
    def __init__(self, path, commit, binary=None):
        self.stack = ExitStack()
        self.items = []
        try:
            self.data = read(path)
            self.rows = parse(self.data, commit)
            self.items.append(self.stack.enter_context(FileSeal(path, 65536)))
            for role in sorted(FILES | TREES):
                name, expected = self.rows[role]
                if role == "binary" and binary is not None: name = str(binary)
                limit = (16 << 30) if role == "iso" else (128 << 20)
                item = TreeSeal(name, expected) if role in TREES else FileSeal(name, limit, expected)
                self.items.append(self.stack.enter_context(item))
            helper, tools = self.path("fixture_helper"), self.path("tools")
            required = [helper / "d11-fixture-helper", helper / RESOURCES / SEED,
                        helper / RESOURCES / POLICY, tools / "wimlib-imagex",
                        tools / "bridgevm-catalog-verify", tools / "bv-file-compare.exe"]
            if any(not p.is_file() or p.is_symlink() for p in required):
                raise ValueError("fixture helper layout differs")
            if self.path("firmware").stat().st_size != 3 << 20:
                raise ValueError("fixture firmware geometry differs")
            if read(helper / "source-commit.txt", 41) != (commit + "\n").encode():
                raise ValueError("fixture helper source differs")
            self.check()
        except BaseException:
            self.close()
            raise

    def path(self, role): return Path(self.rows[role][0])
    def check(self):
        for item in self.items: item.check()
    def close(self): self.stack.close()
    def __enter__(self): return self
    def __exit__(self, *args): self.close()
