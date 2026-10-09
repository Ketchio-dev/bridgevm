#!/usr/bin/env python3
"""Production T22 schema and missing-case boundaries, using tiny host fixtures."""
from __future__ import annotations

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import hashlib
import json
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from a19_interrupt_cases import absent_cases, case_count
import a19_interrupted_restore_receipt as receipt
from a19_collection_fixtures import added_cases, first_case, put
from a19_seed_source_binding_cases import ProductionSourceBinding

spec = spec_from_file_location("production_t22", ROOT / "scripts/live-gates/run-a19-interrupted-restore-tier.py")
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


class ProductionCollection(ProductionSourceBinding, unittest.TestCase):
    def prepared(self, output: Path) -> dict:
        original = first_case(output)
        added_cases(output, original)
        return receipt.initial("collection-fixture", "c" * 40)

    def refused(self, output: Path, value: dict) -> None:
        before = value.copy()
        with self.assertRaises((OSError, ValueError)):
            runner.collect(output, value)
        self.assertEqual(value, before)

    def test_new_receipts_start_with_schema_two_and_absent_cases(self):
        value = receipt.initial("collection-fixture", "c" * 40)
        self.assertEqual(value["schema_version"], 2)
        self.assertEqual({key: value[key] for key in absent_cases()}, absent_cases())

    def test_production_refuses_missing_auxiliary_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            first_case(output)
            value = receipt.initial("collection-fixture", "c" * 40)
            before = value.copy()
            with self.assertRaises((OSError, ValueError)):
                runner.collect(output, value)
            self.assertEqual(value, before)

    def test_complete_collection_produces_three_cases_and_one_four_boot_sample(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value = self.prepared(output)
            runner.collect(output, value)
            self.assertNotEqual(value["snapshot_vars_sha256"], value["preinterrupt_vars_sha256"])
            for member in ("disk", "vars"):
                for prefix in ("postkill", "swap_preinterrupt", "swap_postkill", "create_source", "create_postretry"):
                    self.assertEqual(value[f"{prefix}_{member}_sha256"], value[f"preinterrupt_{member}_sha256"])
            for field in receipt.HASHES:
                if value[field] == "absent":
                    value[field] = "a" * 64
            value.update({"pass": True, "outcome": "completed", "worker_cleanup_verified": True,
                "host_model": "Mac16,9", "macos_version": "26.7", "finished_at": value["started_at"],
                "boots_attempted": 4, "boots_passed": 4, "natural_shutdown_count": 4,
                "interruption_case_count": case_count(value), "run_count": 1, "sample_count": 1})
            self.assertEqual(receipt.validate(value)["interruption_case_count"], 3)
            self.assertFalse(value["claim_eligible"] or value["criterion_pass"] or value["capability_promotion"])

    def test_parent_alias_keeps_cli_invocation_identity_and_resolved_stop_ownership(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            real = root / "real"
            real.mkdir()
            alias = root / "parent-alias"
            alias.symlink_to(real, target_is_directory=True)
            output = alias / "collection"
            value = self.prepared(output)
            runner.collect(output, value)
            self.assertEqual(case_count(value), 3)

    def test_partial_cases_missing_host_proof_and_unexpected_create_destination_refuse(self):
        for missing in ("aux-create/retry.stdout", "aux-swap/postkill-host-digest.txt",
                        "aux-create/source-postretry-host-digest.txt"):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                (output / missing).unlink()
                self.refused(output, value)
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value = self.prepared(output)
            put(output, "aux-create/postkill-manifest.json", "{}")
            self.refused(output, value)

    def test_wrong_raw_pid_access_path_or_context_cannot_supply_a_stop(self):
        changes = [("interrupt-helper-fd.private.log", lambda text: text.replace("p124", "p999")),
                   ("interrupt-helper-fd.private.log", lambda text: text.replace("ar\n", "aw\n")),
                   ("interrupt-helper-fd.private.log", lambda text: text.replace("staging/disk.raw", "wrong/disk.raw")),
                   ("interrupt-helper-context.private.json", lambda text: text.replace('"helper_pid": 124', '"helper_pid": true')),
                   ("interrupt-observation.json", lambda text: text.replace("swap-staged-disk-verify-read", "create-staged-disk-hash-read"))]
        for name, change in changes:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                path = output / "aux-swap" / name
                path.write_text(change(path.read_text()))
                if name.endswith("private.log"):
                    observation = output / "aux-swap/interrupt-observation.json"
                    data = json.loads(observation.read_text())
                    data["stop_fd_log_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
                    observation.write_text(json.dumps(data))
                self.refused(output, value)

    def test_missing_or_claimed_previous_destination_is_not_absence_proof(self):
        for wrong in ("missing", "previous", "wrong-path", "changed-selection"):
            with self.subTest(wrong=wrong), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                state = output / "aux-create/destination-state.private.json"
                if wrong == "missing":
                    state.unlink()
                elif wrong == "changed-selection":
                    context = output / "aux-create/interrupt-helper-context.private.json"
                    context.write_text(context.read_text().replace('"selection_after": "absent"',
                                                                  '"selection_after": "previous"'))
                else:
                    data = json.loads(state.read_text())
                    data["preinterrupt_exists" if wrong == "previous" else "destination_path"] = (
                        True if wrong == "previous" else "/private/other/export.snapshot")
                    state.write_text(json.dumps(data))
                self.refused(output, value)

    def test_context_requires_bounded_unique_regular_owned_records_for_every_case(self):
        for wrong in ("first-missing", "symlink", "duplicate", "oversized", "other-owned-stage"):
            with self.subTest(wrong=wrong), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                context = output / "aux-swap/interrupt-helper-context.private.json"
                if wrong == "first-missing":
                    (output / "interrupt-helper-context.private.json").unlink()
                elif wrong == "symlink":
                    target = output / "context-copy.json"
                    target.write_bytes(context.read_bytes())
                    context.unlink()
                    context.symlink_to(target)
                elif wrong == "duplicate":
                    context.write_text(context.read_text().replace('{', '{"helper_pid": 124,', 1))
                elif wrong == "oversized":
                    context.write_text(context.read_text() + " " * 65_536)
                else:
                    data = json.loads(context.read_text())
                    old = data["staged_disk_path"]
                    data["staged_disk_path"] = old.replace("/swap/", "/unrelated/")
                    context.write_text(json.dumps(data))
                    raw = output / "aux-swap/interrupt-helper-fd.private.log"
                    raw.write_text(raw.read_text().replace(old, data["staged_disk_path"]))
                    observed = output / "aux-swap/interrupt-observation.json"
                    record = json.loads(observed.read_text())
                    record["stop_fd_log_sha256"] = hashlib.sha256(raw.read_bytes()).hexdigest()
                    observed.write_text(json.dumps(record))
                self.refused(output, value)

    def test_duplicate_retained_stop_log_cannot_complete_both_added_cases(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            value = self.prepared(output)
            raw = (output / "aux-swap/interrupt-helper-fd.private.log").read_bytes()
            put(output, "aux-create/interrupt-helper-fd.private.log", raw)
            observed = output / "aux-create/interrupt-observation.json"
            record = json.loads(observed.read_text())
            record["stop_fd_log_sha256"] = hashlib.sha256(raw).hexdigest()
            observed.write_text(json.dumps(record))
            self.refused(output, value)

    def test_first_cli_results_and_hash_logs_are_authenticated_bounded_records(self):
        for wrong in ("arbitrary-result", "incomplete", "wrong-snapshot", "duplicate-json", "oversized-fd", "oversized-hash"):
            with self.subTest(wrong=wrong), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                result = output / "restore-retry.json"
                if wrong == "arbitrary-result":
                    result.write_text("restore")
                elif wrong == "incomplete":
                    result.write_text(result.read_text().replace('"complete": true', '"complete": 1'))
                elif wrong == "wrong-snapshot":
                    result.write_text(result.read_text().replace("latest.snapshot", "other.snapshot"))
                elif wrong == "duplicate-json":
                    result.write_text(result.read_text().replace('{', '{"complete": true,', 1))
                elif wrong == "oversized-fd":
                    (output / "interrupt-helper-fd.private.log").write_text("x" * 65_537)
                else:
                    (output / "pre-interrupt-vars.sha256").write_text(" " * 129)
                self.refused(output, value)

    def test_noop_retry_mixed_pair_or_changed_source_refuses_even_when_host_agrees(self):
        for case, name, source in (("aux-swap", "postretry", "preinterrupt"),
                                  ("aux-swap", "postkill", "postretry"),
                                  ("aux-create", "source-postretry", "invalid")):
            with self.subTest(case=case, name=name), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                root = output / case
                data = ((root / f"{source}-digest.txt").read_text() if source != "invalid"
                        else (root / "source-digest.txt").read_text().replace(
                            (root / "source-digest.txt").read_text().splitlines()[1], "disk_sha256 " + "f" * 64))
                for suffix in ("-digest.txt", "-host-digest.txt"):
                    (root / (name + suffix)).write_text(data)
                self.refused(output, value)

    def test_retry_output_and_manifest_must_describe_same_identity_sizes_and_hashes(self):
        changes = [("retry.stdout", lambda text: text.replace("vm_id a19-interrupted-create", "vm_id wrong-vm")),
                   ("retry.stdout", lambda text: text.replace("format_version 1", "format_version 2")),
                   ("retry-manifest.json", lambda text: text.replace('"format_version": 1', '"format_version": true')),
                   ("retry-manifest.json", lambda text: text.replace('"disk_bytes": 14', '"disk_bytes": 15'))]
        for name, change in changes:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                value = self.prepared(output)
                path = output / "aux-create" / name
                original = path.read_text()
                changed = change(original)
                self.assertNotEqual(changed, original)
                path.write_text(changed)
                self.refused(output, value)


if __name__ == "__main__":
    unittest.main()
