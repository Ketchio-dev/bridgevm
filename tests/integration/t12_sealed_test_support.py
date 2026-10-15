"""Owned synthetic T12 repository, assets and queue; no native guest or signer."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TIER = "t12-bridgevm-pc-nvme-block"
IMAGE = hashlib.sha256(b"BRIDGEVM" + bytes(1048576 - 8)).hexdigest()
VARS = hashlib.sha256(bytes([255]) * 65536).hexdigest()

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

class Fixture:
    def __init__(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name).resolve()
        self.repo = self.base / "repo"
        self.scripts = self.repo / "scripts/live-gates"
        self.scripts.mkdir(parents=True)
        for path in (ROOT / "scripts/live-gates").iterdir():
            if path.is_file() and (path.suffix in (".sh", ".py") or path.name == "bridgevm-live"):
                shutil.copy2(path, self.scripts / path.name)
        integration = self.repo / "tests/integration"
        integration.mkdir(parents=True)
        shutil.copy2(ROOT / "tests/integration/bridgevm-pc-variable-process-persistence.sh", integration)
        self.assets = self.base / "assets"; self.assets.mkdir()
        self.firmware = self.assets / "firmware.fd"; self.firmware.write_bytes(b"synthetic pinned firmware")
        pin = self.repo / "crates/bridgevm-hvf/firmware/bridgevm-pc-dxe-entry.sha256"
        pin.parent.mkdir(parents=True); pin.write_text(digest(self.firmware) + "\n")
        self.marker = self.base / "forbidden-build"
        builder = self.repo / "scripts/build-bridgevm-pc-dxe-entry-firmware.sh"
        builder.write_text('#!/bin/sh\nprintf called > "' + str(self.marker) + '"\nexit 79\n'); builder.chmod(0o700)
        self.binary = self.assets / "runner"; self.binary.write_text('#!/bin/sh\nexit 78\n'); self.binary.chmod(0o700)
        self.env = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null")
        self.git("init", "-q"); self.git("config", "maintenance.auto", "false"); self.git("add", ".")
        self.git("-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid", "commit", "-qm", "synthetic T12 fixture")
        self.commit = self.git("rev-parse", "HEAD")
        self.queue = self.base / "queue"
        self.env["BRIDGEVM_LIVE_ROOT"] = str(self.queue)
        self.manifest = self.base / "manifest.tsv"
        self.rows = {"schema": ["bridgevm.t12-sealed-inputs.v1"], "binary_source_commit": [self.commit],
            "binary_profile": ["debug"], "binary_features": ["default"], "rust_toolchain": ["1.97.0"],
            "binary": [str(self.binary), digest(self.binary)], "firmware": [str(self.firmware), digest(self.firmware)],
            "image_recipe": ["bridgevm-pc-nvme-marker-v1"], "image_sha256": [IMAGE],
            "initial_vars_recipe": ["bridgevm-pc-erased-vars-v1"], "initial_vars_sha256": [VARS]}
        self.write_manifest()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], env=self.env, stderr=subprocess.DEVNULL).decode().strip()

    def write_manifest(self):
        self.manifest.write_text("".join("\t".join([key, *value]) + "\n" for key, value in self.rows.items()))

    def command(self, *args):
        return subprocess.run(["/bin/bash", str(self.scripts / "bridgevm-live"), *args], env=self.env, capture_output=True, text=True, timeout=20)

    def submit(self, name):
        return self.command("submit", TIER, "--sha", self.commit, "--input-manifest", str(self.manifest), "--job-id", name)

    def close(self): self.temp.cleanup()
