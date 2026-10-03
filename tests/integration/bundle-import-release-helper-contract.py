#!/usr/bin/env python3
"""Prove the actual release CLI ignores PATH helpers and refuses recorded programs.

Fixtures contain only generated tiny qcow2 files and disposable marker scripts.
Missing fixed-path host tools fail the gate; no guest or live VM is launched.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import unittest

from bundle_import_portability_support import ROOT, digests, run

FIXTURE_BINARY = None
EVIDENCE_DIR = None


def trusted_qemu() -> Path:
    candidates = (("/opt/homebrew/bin/qemu-img", "/usr/local/bin/qemu-img")
                  if platform.system() == "Darwin" else ("/usr/bin/qemu-img",))
    for candidate in candidates:
        path = Path(candidate)
        if path.is_file() and os.access(path, os.X_OK):
            return path
    raise AssertionError(f"required fixed-path qemu-img is absent: {candidates}")


def release_cli() -> Path:
    if FIXTURE_BINARY is not None:
        return Path(FIXTURE_BINARY).resolve(strict=True)
    toolchain = os.environ.get("BRIDGEVM_CHECK_TOOLCHAIN", "+1.97.0")
    run("cargo", toolchain, "build", "--release", "--locked", "--jobs", "2",
        "-p", "bridgevm-cli", timeout=600)
    metadata = json.loads(run("cargo", toolchain, "metadata", "--no-deps",
                              "--format-version", "1", "--locked"))
    return Path(metadata["target_directory"]) / "release/bridgevm"


def invoke(cli: Path, *args: str | Path, env=None):
    return subprocess.run([str(cli), *map(str, args)], cwd=ROOT, env=env,
                          text=True, capture_output=True, timeout=60)


def script(path: Path, marker: Path, status: int):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('#!/bin/sh\nprintf "fixture helper invoked\\n" > '
                    '"$BRIDGEVM_RELEASE_FIXTURE_MARKER"\n' f'exit {status}\n')
    path.chmod(0o700)
    env = dict(os.environ)
    env["BRIDGEVM_RELEASE_FIXTURE_MARKER"] = str(marker)
    return env


def evidence(label: str, marker: Path, result, binary: Path):
    if EVIDENCE_DIR is None:
        return
    output = Path(EVIDENCE_DIR)
    output.mkdir(parents=True, exist_ok=True)
    marker_bytes = marker.read_bytes() if marker.exists() else None
    if marker_bytes is not None:
        with (output / f"{label}-marker.txt").open("xb") as retained:
            retained.write(marker_bytes)
    value = {"return_code": result.returncode, "marker_exists": marker_bytes is not None,
             "marker_sha256": hashlib.sha256(marker_bytes).hexdigest() if marker_bytes else None,
             "release_cli_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
    # Each retained stream carries at most 8 KiB of the observed UTF-8 bytes.
    for name in ("stdout", "stderr"):
        raw = getattr(result, name).encode("utf-8")
        value[name] = raw[:8192].decode("utf-8", errors="ignore")
        value[f"{name}_bytes"] = len(raw)
        value[f"{name}_truncated"] = len(raw) > 8192
    with (output / f"{label}-result.json").open("x") as retained:
        json.dump(value, retained, indent=2)


class ReleaseHelperContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.qemu = trusted_qemu()
        cls.cli = release_cli()
        print(f"trusted_fixture_tool={cls.qemu}; {run(cls.qemu, '--version').splitlines()[0]}",
              flush=True)
        print("release_cli_sha256=" + hashlib.sha256(cls.cli.read_bytes()).hexdigest(),
              flush=True)

    def bundle(self, root: Path):
        source = root / "source"
        result = invoke(self.cli, "--store", source, "create", "release-source",
                        "--os", "ubuntu", "--arch", "x86_64", "--mode", "compatibility",
                        "--disk", "8M", "--disk-format", "qcow2")
        self.assertEqual(result.returncode, 0, result.stderr)
        bundle = source / "vms/release-source.vmbridge"
        primary = bundle / "disks/root.qcow2"
        run(self.qemu, "create", "-f", "qcow2", primary, "8M")
        info = json.loads(run(self.qemu, "info", "--output=json", primary))
        self.assertEqual(info["format"], "qcow2")
        self.assertEqual(info["virtual-size"], 8 * 1024 * 1024)
        return source, bundle

    def test_release_import_ignores_poisoned_path_qemu_img(self):
        with tempfile.TemporaryDirectory(prefix="bridgevm-release-helper-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            _, bundle = self.bundle(root)
            original = digests(bundle)
            marker = root / "poison.marker"
            fake_bin = root / "poisoned bin"
            env = script(fake_bin / "qemu-img", marker, 73)
            env["PATH"] = str(fake_bin)
            target = root / "target"
            result = invoke(self.cli, "--store", target, "import", bundle,
                            "--name", "release-copy", env=env)
            evidence("path-poison", marker, result, self.cli)
            self.assertFalse(marker.exists(), f"release ran PATH poison: {result.stderr}")
            self.assertEqual(result.returncode, 0, result.stderr)
            imported = target / "vms/release-copy.vmbridge"
            self.assertTrue((imported / "metadata/import.json").is_file())
            info = json.loads(run(self.qemu, "info", "--output=json", imported / "disks/root.qcow2"))
            self.assertEqual(info["virtual-size"], 8 * 1024 * 1024)
            self.assertEqual(digests(bundle), original)

    def test_release_snapshot_refuses_unapproved_recorded_program(self):
        with tempfile.TemporaryDirectory(prefix="bridgevm-recorded-helper-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            source, bundle = self.bundle(root)
            result = invoke(self.cli, "--store", source, "snapshot", "create",
                            "release-source", "before", "--kind", "disk")
            self.assertEqual(result.returncode, 0, result.stderr)
            record = bundle / "metadata/snapshot-disks/before.json"
            metadata = json.loads(record.read_text())
            self.assertTrue(Path(metadata["backing_path"]).is_file())
            self.assertFalse(Path(metadata["overlay_path"]).exists())
            marker = root / "recorded.marker"
            recorded = root / "unapproved helper"
            env = script(recorded, marker, 0)
            metadata["create_command"][0] = str(recorded)
            record.write_text(json.dumps(metadata))
            original = digests(bundle)
            result = invoke(self.cli, "--store", source, "snapshot", "disk-create",
                            "release-source", "before", env=env)
            evidence("recorded-program", marker, result, self.cli)
            self.assertFalse(marker.exists(), "release ran the unapproved recorded program")
            self.assertNotEqual(result.returncode, 0, "release accepted an unapproved recorded program")
            self.assertTrue(result.stderr.strip(), "refusal lost its reason")
            self.assertEqual(digests(bundle), original)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", help="already built release artifact for before/after evidence")
    parser.add_argument("--evidence-dir", help="retain marker bytes and bounded fixture results")
    args, remaining = parser.parse_known_args()
    FIXTURE_BINARY = args.binary
    EVIDENCE_DIR = args.evidence_dir
    unittest.main(argv=[sys.argv[0], *remaining], verbosity=2)
