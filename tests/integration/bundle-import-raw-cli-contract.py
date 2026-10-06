#!/usr/bin/env python3
"""Import owned raw-only directory/tar bundles without a QEMU helper on PATH.

No image tool, guest, native backend or live VM runs in these fixtures.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from bundle_import_portability_support import (
    ROOT, binaries, digests, export_metadata, payload_digest, read_json,
)

NAME = "raw-source"
FIXTURE_BINARY = None
EVIDENCE_DIR = None


def invoke(cli: Path, store: Path, *args: str | Path, env=None):
    return subprocess.run([str(cli), "--store", str(store), *map(str, args)],
                          cwd=ROOT, env=env, text=True, capture_output=True, timeout=60)


def evidence(label: str, result, cli: Path, no_helper: bool):
    if EVIDENCE_DIR is None:
        return
    output = Path(EVIDENCE_DIR)
    output.mkdir(parents=True, exist_ok=True)
    value = {"return_code": result.returncode, "qemu_img_on_path": not no_helper,
             "debug_cli_sha256": hashlib.sha256(cli.read_bytes()).hexdigest()}
    for name in ("stdout", "stderr"):
        raw = getattr(result, name).encode("utf-8")
        value[name] = raw[:8192].decode("utf-8", errors="ignore")
        value[f"{name}_bytes"] = len(raw)
        value[f"{name}_truncated"] = len(raw) > 8192
    with (output / f"{label}-result.json").open("x") as retained:
        json.dump(value, retained, indent=2)


class RawBundleContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cli = (Path(FIXTURE_BINARY).resolve(strict=True) if FIXTURE_BINARY
                   else binaries()[0].resolve(strict=True))
        print("debug_cli_sha256=" + hashlib.sha256(cls.cli.read_bytes()).hexdigest(),
              flush=True)

    def success(self, store: Path, *args: str | Path, env=None) -> str:
        result = invoke(self.cli, store, *args, env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def scenario(self, archive: str):
        with tempfile.TemporaryDirectory(prefix="bridgevm-raw-import-", dir="/tmp") as temporary:
            root = Path(temporary).resolve()
            source = root / "source store"
            self.success(source, "create", NAME, "--os", "ubuntu", "--arch", "x86_64",
                         "--mode", "compatibility", "--disk", "8M", "--disk-format", "raw")
            bundle = source / f"vms/{NAME}.vmbridge"
            primary = bundle / "disks/root.raw"
            raw = bytearray(8 * 1024 * 1024)
            raw[:4096], raw[4096:8192], raw[-4096:] = (bytes([17]) * 4096,
                                                     bytes([34]) * 4096, bytes([51]) * 4096)
            primary.write_bytes(raw)
            active = {"source": "primary", "path": str(primary), "format": "raw",
                      "exists": True, "activated_at_unix": 123456}
            (bundle / "metadata/active-disk.json").write_text(json.dumps(active))
            (bundle / "metadata/retained-evidence.json").write_text('{"fixture":"raw-only"}\n')
            (bundle / "metadata/machine-identifier.bin").write_bytes(b"synthetic raw identity")
            original = digests(bundle)
            exported = root / ("old export.tar" if archive == "tar" else "old export.vmbridge")
            self.success(source, "export", NAME, "--output", exported)
            export_receipt = export_metadata(exported)
            self.assertEqual(export_receipt["archive_format"], archive)
            withdrawn_source = root / "withdrawn source"
            source.rename(withdrawn_source)
            relocated = root / ("relocated.tar" if archive == "tar" else "relocated.vmbridge")
            exported.rename(relocated)
            input_digest = payload_digest(relocated)
            self.assertFalse(source.exists())
            self.assertFalse(exported.exists())
            self.assertFalse(primary.exists())
            empty_path = root / "empty PATH"
            empty_path.mkdir()
            env = dict(os.environ, PATH=str(empty_path))
            no_helper = shutil.which("qemu-img", path=env["PATH"]) is None
            self.assertTrue(no_helper)
            copies = []
            for name in (NAME, "raw-copy"):
                target = root / f"target {name}"
                extra = () if name == NAME else ("--name", name)
                result = invoke(self.cli, target, "import", relocated, *extra, env=env)
                evidence(f"{archive}-{name}", result, self.cli, no_helper)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f"Imported {name}", result.stdout)
                self.assertEqual(payload_digest(relocated), input_digest)
                copies.append((target, target / f"vms/{name}.vmbridge", name))
            withdrawn_input = root / "withdrawn input"
            relocated.rename(withdrawn_input)
            self.assertFalse(relocated.exists())
            preserved_source = withdrawn_source / f"vms/{NAME}.vmbridge"
            for target, imported, name in copies:
                disk = imported / "disks/root.raw"
                self.assertEqual(disk.read_bytes(), raw)
                self.assertEqual(read_json(imported / "metadata/active-disk.json"),
                                 dict(active, path=str(disk)))
                for relative in ("metadata/state.json", "metadata/guest-tools-token.json",
                                 "metadata/retained-evidence.json", "metadata/machine-identifier.bin"):
                    self.assertEqual(digests(imported)[relative], original[relative], relative)
                manifest = (imported / "manifest.yaml").read_text()
                self.assertIn(f"name: {name}\n", manifest)
                self.assertIn("path: disks/root.raw\n", manifest)
                self.assertIn("format: raw\n", manifest)
                receipt = read_json(imported / "metadata/import.json")
                self.assertEqual(receipt["original_name"], NAME)
                self.assertEqual(receipt["vm"], name)
                self.assertEqual(receipt["manifest_identity_rewritten"], name != NAME)
                self.assertTrue(receipt["metadata_preserved"])
                self.assertEqual(Path(receipt["source"]), relocated)
                self.assertEqual(Path(receipt["output"]), imported)
                self.assertEqual(read_json(imported / "metadata/export.json"), export_receipt)
                chain = self.success(target, "snapshot", "chain", name, env=env)
                self.assertIn(f"Active disk: {disk}", chain)
                self.assertIn("Active disk format: raw", chain)
                self.assertIn("Active disk ready: true", chain)
                self.assertNotIn(str(primary), chain)
            untouched = digests(copies[1][1])
            selected = copies[0][1] / "disks/root.raw"
            with selected.open("r+b") as stream:
                stream.seek(4096)
                stream.write(bytes([85]) * 4096)
            self.assertEqual(selected.read_bytes()[4096:8192], bytes([85]) * 4096)
            self.assertEqual(digests(copies[1][1]), untouched)
            self.assertEqual(digests(preserved_source), original)
            self.assertEqual(payload_digest(withdrawn_input), input_digest)

    def test_directory_raw_import_without_qemu(self):
        self.scenario("directory")

    def test_tar_raw_import_without_qemu(self):
        self.scenario("tar")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", help="already built debug CLI for before/after evidence")
    parser.add_argument("--evidence-dir", help="retain bounded import results")
    args, remaining = parser.parse_known_args()
    FIXTURE_BINARY, EVIDENCE_DIR = args.binary, args.evidence_dir
    unittest.main(argv=[sys.argv[0], *remaining], verbosity=2)
