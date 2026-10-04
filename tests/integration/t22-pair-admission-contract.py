#!/usr/bin/env python3
"""Reject partial, unknown and unbound encryption observations."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from t22_pair_fixtures import report, gpt
from t22_pair_admission import validate_query, read_query, screen_disk, METHODS, shutdown_observed
from hvf_terminal_report import FOOTER, SERIAL
from t17_terminal_report_tail import BANNER


class Admission(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-t22-admission-")
        self.root = Path(self.temp.name)

    def tearDown(self): self.temp.cleanup()

    def check(self, value): return validate_query(value, "1" * 32, "2" * 64)

    def test_complete_os_data_and_efi_observation(self):
        self.assertEqual(self.check(report()), {"fixed_volume_count": 3, "decrypted_ntfs_volume_count": 2})

    def test_protection_off_does_not_admit_encrypted_or_partial_data(self):
        for key in ("conversion_status", "encryption_method"):
            for state in range(1, 8):
                value = report(); value["volumes"][1][key] = state
                with self.subTest(key=key, state=state), self.assertRaises(ValueError): self.check(value)

    def test_every_method_requires_exact_integer_zero(self):
        for key in METHODS:
            for bad in (None, False, True, "0", 0.0, 2150694912):
                value = report(); value["volumes"][1][key] = bad
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError): self.check(value)

    def test_os_and_unique_fixed_volume_coverage(self):
        values = []
        value = report(); value["volumes"].pop(0); values.append(value)
        value = report(); value["volumes"].append(copy.deepcopy(value["volumes"][1])); values.append(value)
        value = report(); value["volumes"][1]["drive_letter"] = "C:"; values.append(value)
        value = report(); value["volumes"][2]["filesystem"] = "unknown"; values.append(value)
        value = report(); value["volumes"][2]["conversion_status"] = 0; values.append(value)
        value = report(); value["volumes"][1]["drive_type"] = True; values.append(value)
        value = report(); value["volumes"][1]["device_id"] = "\\\\?\\Volume{" + "-" * 36 + "}\\"; values.append(value)
        for value in values:
            with self.subTest(value=value), self.assertRaises(ValueError): self.check(value)

    def test_nonce_schema_hash_and_unknown_keys_refuse(self):
        for key, bad in (("nonce", "3" * 32), ("script_sha256", "4" * 64), ("schema", "other"), ("extra", True)):
            value = report(); value[key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError): self.check(value)

    def test_raw_result_digest_duplicate_and_symlink_refuse(self):
        target = self.root / ("result-" + "1" * 32 + ".json")
        done = target.with_suffix(".done")
        data = json.dumps(report()).encode(); target.write_bytes(data)
        done.write_bytes(hashlib.sha256(data).hexdigest().encode())
        self.assertEqual(read_query(self.root, "1" * 32, "2" * 64)[0]["decrypted_ntfs_volume_count"], 2)
        done.write_bytes(b"0" * 64)
        with self.assertRaises(ValueError): read_query(self.root, "1" * 32, "2" * 64)
        data = data[:-1] + b',"nonce":"' + b"1" * 32 + b'"}'
        target.write_bytes(data); done.write_bytes(hashlib.sha256(data).hexdigest().encode())
        with self.assertRaises(ValueError): read_query(self.root, "1" * 32, "2" * 64)
        saved = target.with_suffix(".saved"); target.rename(saved); target.symlink_to(saved)
        with self.assertRaises(OSError): read_query(self.root, "1" * 32, "2" * 64)

    def test_invalid_filename_identity_refuses_before_read(self):
        for nonce, digest in (("../outside", "2" * 64), ("1" * 32 + "\n", "2" * 64),
                              (None, "2" * 64), ("1" * 32, True), ("1" * 32, "")):
            with self.subTest(nonce=nonce, digest=digest), patch("t22_pair_admission.read_bounded_regular", side_effect=AssertionError("must not read")), self.assertRaises(ValueError):
                read_query(self.root, nonce, digest)

    def test_gpt_screen_refuses_fve_unknown_and_overlap(self):
        disk = self.root / "disk.raw"
        gpt(disk); self.assertEqual(screen_disk(disk), {"partition_count": 2, "ntfs_partition_count": 1})
        for options in ({"oem": b"-FVE-FS-"}, {"oem": b"OTHER   "}, {"unknown": True}, {"overlap": True}):
            gpt(disk, **options)
            with self.subTest(options=options), self.assertRaises(ValueError): screen_disk(disk)

    def test_shutdown_requires_host_framed_system_off_and_writeback(self):
        log = self.root / "run.log"
        records = b"host media: NVMe disk written back: owned.raw (512 bytes)\n"
        def raw(stop, media):
            return BANNER + b"stop: " + stop + b"\n" + media + b"serial raw bytes: 00000000 output bytes: 00000000" + SERIAL + FOOTER
        valid = raw(b"PSCI 0x84000008 (system off)", records)
        log.write_bytes(valid); self.assertEqual(shutdown_observed(0, log), hashlib.sha256(valid).hexdigest())
        for status, data in ((True, valid), (1, valid), (0, raw(b"watchdog", records)), (0, raw(b"PSCI 0x84000008 (system off)", b""))):
            log.write_bytes(data)
            with self.subTest(status=status, data=data), self.assertRaises(ValueError): shutdown_observed(status, log)


if __name__ == "__main__": unittest.main()
