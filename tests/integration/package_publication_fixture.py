"""Owned synthetic apps; real cp/tar, stub codesign/hdiutil (never sign or mount)."""
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile

REPO = Path(os.environ.get("PACKAGING_TEST_ROOT", Path(__file__).resolve().parents[2]))
STUB = r'''
import json, os, pathlib, subprocess, sys, tempfile
tool = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
mode = os.environ.get("PACKAGE_TEST_MODE", "pass")
with open(os.environ["PACKAGE_TEST_LOG"], "a") as log:
    log.write(json.dumps([tool, *args]) + "\n")
def race():
    out = pathlib.Path(os.environ["PACKAGE_TEST_OUT"])
    kind = os.environ.get("PACKAGE_TEST_RACE", "file")
    if kind == "file": out.write_bytes(b"concurrent artifact")
    elif kind == "directory":
        out.mkdir(); (out / "sentinel").write_bytes(b"concurrent artifact")
    else: out.symlink_to(os.environ["PACKAGE_TEST_TARGET"] if kind == "symlink" else "absent-target")
if tool == "codesign":
    initial = args[-1] == os.environ["PACKAGE_TEST_APP"]
    if initial and mode == "input-fail": sys.exit(61)
    if not initial and mode == "verify-fail": sys.exit(62)
    if not initial and mode == "race": race()
elif tool == "hdiutil":
    archive = pathlib.Path(args[-1])
    if args[0] == "create":
        archive.write_bytes(b"synthetic disk image")
        if mode == "create-fail": sys.exit(73)
    elif args[0] == "verify":
        if mode == "verify-fail": sys.exit(74)
        if mode == "race": race()
        assert archive.read_bytes() == b"synthetic disk image"
    else: sys.exit(75)
elif tool == "tar":
    if "-czf" in args and mode == "create-fail":
        pathlib.Path(args[args.index("-czf") + 1]).write_bytes(b"partial archive")
        sys.exit(76)
    if "-xzf" in args and mode == "extract-fail": sys.exit(77)
    sys.exit(subprocess.call([os.environ["PACKAGE_TEST_TAR"], *args]))
elif tool == "mktemp":
    assert args == ["-d"], args
    print(tempfile.mkdtemp(dir=os.environ["PACKAGE_TEST_TMP"]))
'''


def snapshot(path):
    try:
        info = path.lstat()
    except FileNotFoundError:
        return None
    value = [info.st_ino, info.st_dev, stat.S_IMODE(info.st_mode)]
    if path.is_symlink():
        value.append(os.readlink(path))
    elif path.is_dir():
        value.append({child.name: snapshot(child) for child in sorted(path.iterdir())})
    else:
        value.append(path.read_bytes())
    return value


class Fixture:
    def __init__(self, kind):
        self.temp = tempfile.TemporaryDirectory(prefix="package-publication-")
        self.root = Path(self.temp.name)
        self.kind = kind
        self.app = self.root / "input" / "BridgeVM.app"
        (self.app / "Contents").mkdir(parents=True)
        self.payload = self.app / "Contents" / "payload"
        self.payload.write_bytes(b"original app bytes\n")
        self.payload.chmod(0o751)
        (self.app / "Contents" / "payload-link").symlink_to("payload")
        self.output = self.root / ("result.dmg" if kind == "dmg" else "result.tar.gz")
        self.target = self.root / "unrelated-target"
        self.target.write_bytes(b"unrelated bytes")
        self.log = self.root / "commands.jsonl"
        self.tmp = self.root / "tmp"
        self.tmp.mkdir()
        bins = self.root / "bin"
        bins.mkdir()
        for tool in ("codesign", "hdiutil", "tar", "mktemp"):
            path = bins / tool
            path.write_text(f"#!{sys.executable}\n" + STUB)
            path.chmod(0o755)
        self.env = dict(os.environ, PATH=str(bins) + os.pathsep + os.environ["PATH"],
                        TMPDIR=str(self.tmp), PACKAGE_TEST_TMP=str(self.tmp),
                        PACKAGE_TEST_LOG=str(self.log), PACKAGE_TEST_TAR=shutil.which("tar"),
                        PACKAGE_TEST_TARGET=str(self.target))

    def run(self, mode="pass", app=None, race="file"):
        env = dict(self.env, PACKAGE_TEST_MODE=mode, PACKAGE_TEST_APP=str(app or self.app),
                   PACKAGE_TEST_OUT=str(self.output), PACKAGE_TEST_RACE=race)
        script = REPO / "scripts" / ("package-dmg.sh" if self.kind == "dmg" else "package-tarball.sh")
        return subprocess.run(["/bin/bash", str(script), "--app", str(app or self.app),
                               "--output", str(self.output)], env=env, text=True,
                              capture_output=True, timeout=30)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def close(self):
        self.temp.cleanup()
