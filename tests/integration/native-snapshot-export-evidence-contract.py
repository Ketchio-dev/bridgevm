#!/usr/bin/env python3
"""Contracts for evidence consumed by the A19 live exported-pair boot."""
from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PATH = ROOT / "scripts/live-gates/native_snapshot_export_evidence.py"
SPEC = importlib.util.spec_from_file_location("native_export_evidence", PATH)
EVIDENCE = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(EVIDENCE)


class NativeSnapshotExportEvidenceContract(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[Path, Path, Path, str]:
        vm_id = "a19-native-cli-live"
        library = root / "library"
        export = root / "export.snapshot"; export.mkdir()
        disk, variables = export / "disk.raw", export / "vars.fd"
        disk.write_bytes(b"restored-disk"); variables.write_bytes(b"restored-vars")
        manifest = {
            "format_version": 1, "vm_id": vm_id,
            "disk_bytes": disk.stat().st_size,
            "disk_sha256": hashlib.sha256(disk.read_bytes()).hexdigest(),
            "vars_bytes": variables.stat().st_size,
            "vars_sha256": hashlib.sha256(variables.read_bytes()).hexdigest(),
        }
        (export / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        result = root / "export.json"
        result.write_text(json.dumps({
            "schema": "bridgevm.app-snapshot.v1", "command": "export", "vmID": vm_id,
            "libraryPath": str(library), "snapshotPath": str(export), "complete": True,
        }), encoding="utf-8")
        return result, export, library, vm_id

    def test_verified_pair_produces_path_free_hash_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            result, export, library, vm_id = self.fixture(Path(temporary))
            value = EVIDENCE.verify(result, export, vm_id, library)
            self.assertEqual(value["schema"], "bridgevm.native-snapshot-export-evidence.v1")
            self.assertEqual(value["disk_sha256"], hashlib.sha256(b"restored-disk").hexdigest())
            evidence = Path(temporary) / "evidence.json"
            evidence.write_text(json.dumps(value), encoding="utf-8")
            self.assertEqual(EVIDENCE.load_evidence(evidence, vm_id), value)
            self.assertEqual(EVIDENCE.receipt_fields(value)["exported_disk_sha256"], value["disk_sha256"])
            self.assertFalse(any("/" in item for item in value.values()))

    def test_result_manifest_bytes_and_file_set_fail_closed(self):
        mutations = ("result-identity", "manifest-hash", "disk-bytes", "extra-file", "symlink-vars")
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); result, export, library, vm_id = self.fixture(root)
                if mutation == "result-identity":
                    value = json.loads(result.read_text()); value["vmID"] = "other"; result.write_text(json.dumps(value))
                elif mutation == "manifest-hash":
                    path = export / "manifest.json"; value = json.loads(path.read_text()); value["disk_sha256"] = "0" * 64; path.write_text(json.dumps(value))
                elif mutation == "disk-bytes":
                    (export / "disk.raw").write_bytes(b"changed")
                elif mutation == "extra-file":
                    (export / "private.raw").write_bytes(b"secret")
                else:
                    path = export / "vars.fd"; path.unlink(); path.symlink_to(export / "disk.raw")
                with self.assertRaises((OSError, ValueError)):
                    EVIDENCE.verify(result, export, vm_id, library)

    def test_command_refuses_invalid_export(self):
        with tempfile.TemporaryDirectory() as temporary:
            result, export, library, vm_id = self.fixture(Path(temporary))
            (export / "disk.raw").write_bytes(b"RESTORED-DISK")
            completed = subprocess.run(
                [sys.executable, str(PATH), str(result), str(export), vm_id, str(library)],
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertEqual(completed.stdout, "")
            self.assertIn("does not match its manifest", completed.stderr)

    def test_shell_routes_cli_export_and_selects_verified_pair(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); cli = root / "fake-cli.py"
            cli.write_text("""#!/usr/bin/env python3
import hashlib,json,pathlib,sys
assert sys.argv[1:3] == ['app','snapshot-export'] and sys.argv[5] == '--library' and sys.argv[7] == '--json'
vm,out,library=sys.argv[3],pathlib.Path(sys.argv[4]),sys.argv[6]; out.mkdir()
disk,vars=out/'disk.raw',out/'vars.fd'; disk.write_bytes(b'disk'); vars.write_bytes(b'vars')
(out/'manifest.json').write_text(json.dumps({'format_version':1,'vm_id':vm,'disk_bytes':4,'disk_sha256':hashlib.sha256(b'disk').hexdigest(),'vars_bytes':4,'vars_sha256':hashlib.sha256(b'vars').hexdigest()}))
print(json.dumps({'schema':'bridgevm.app-snapshot.v1','command':'export','vmID':vm,'libraryPath':library,'snapshotPath':str(out),'complete':True}))
"""); cli.chmod(0o700)
            work, output, library = root / "work", root / "output", root / "library"
            work.mkdir(); output.mkdir(); library.mkdir()
            script = """set -euo pipefail
REPO=$1; source "$REPO/scripts/native-snapshot-export-live.sh"
native_snapshot_export_and_select "$2" a19-native-cli-live "$3" "$4" "$5"
printf '%s\n%s\n' "$WORK_DISK" "$WORK_VARS"
"""
            completed = subprocess.run(
                ["bash", "-c", script, "fixture", str(ROOT), str(cli), str(library), str(work), str(output)],
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(completed.stdout.splitlines(), [str(work / "export.snapshot/disk.raw"), str(work / "export.snapshot/vars.fd")])
            self.assertEqual(EVIDENCE.load_evidence(output / "export-evidence.json", "a19-native-cli-live")["disk_sha256"], hashlib.sha256(b"disk").hexdigest())

    def test_retained_evidence_rejects_missing_or_unbound_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); result, export, library, vm_id = self.fixture(root)
            value = EVIDENCE.verify(result, export, vm_id, library)
            for mutation in ({key: item for key, item in value.items() if key != "vars_sha256"},
                             {**value, "vm_id": "other"}, {**value, "disk_sha256": "absent"}):
                evidence = root / ("evidence-" + str(len(list(root.iterdir()))) + ".json")
                evidence.write_text(json.dumps(mutation), encoding="utf-8")
                with self.assertRaises(ValueError):
                    EVIDENCE.load_evidence(evidence, vm_id)


if __name__ == "__main__":
    unittest.main()
