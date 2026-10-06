#!/usr/bin/env python3
"""Run the real preparer with an owned subprocess standing in for a boot."""
from contextlib import ExitStack
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from t22_pair_fixtures import ROOT, COMMIT, PairFixture
from t22_pair_provenance import admit, file_hash
from t22_pair_runtime import execute
from t22_pair_publication import publish, record
from retained_windows_identity import directory_identity

spec = importlib.util.spec_from_file_location("pair_preparer", ROOT / "scripts/live-gates/prepare-t22-owned-pair.py")
preparer = importlib.util.module_from_spec(spec); spec.loader.exec_module(preparer)


class Preparation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-t22-prepare-")
        self.root = Path(self.temp.name).resolve(); self.root.chmod(0o700)
        self.fixture = PairFixture(self.root)
        launcher = ROOT / "tests/fixtures/t22-owned-pair-boot.py"
        self.fixture.wrapper.write_text("#!/bin/bash\nexec " + shlex.quote(sys.executable) + " " + shlex.quote(str(launcher)) + ' "$@"\n')
        self.fixture.refresh()
        self.output = self.root / "prepared"
        self.args = __import__("argparse").Namespace(commit=COMMIT, job_id="owned-fixture",
            input_manifest=self.fixture.manifest, input_sha256=self.fixture.manifest_hash,
            origin_manifest=self.fixture.origin_path, origin_sha256=self.fixture.origin_hash, output=self.output)
        self.originals = {k: file_hash(p) for k, p in (("image", self.fixture.disk), ("vars", self.fixture.vars))}

    def tearDown(self):
        for path in self.root.rglob("*"):
            if not path.is_symlink(): path.chmod(0o700 if path.is_dir() else 0o600)
        self.temp.cleanup()

    def run_preparer(self, mode="normal", more=None):
        real_run, real_output = subprocess.run, subprocess.check_output
        def run(command, **kwargs):
            if command[0] == "/usr/bin/codesign": return subprocess.CompletedProcess(command, 0)
            return real_run(command, **kwargs)
        def output(command, **kwargs):
            if command == ["/usr/bin/git", "--no-optional-locks", "rev-parse", "HEAD"]: return COMMIT + "\n"
            if command == ["/usr/bin/git", "--no-optional-locks", "status", "--porcelain", "--untracked-files=all"]: return ""
            return real_output(command, **kwargs)
        with ExitStack() as stack:
            stack.enter_context(patch("subprocess.run", side_effect=run))
            stack.enter_context(patch("subprocess.check_output", side_effect=output))
            stack.enter_context(patch("platform.system", return_value="Darwin"))
            stack.enter_context(patch("platform.machine", return_value="arm64"))
            shell_override = self.root / "forbidden-bash-env"; shell_override.write_text('touch "' + str(self.root / "override-executed") + '"\n')
            stack.enter_context(patch.dict(os.environ, {"T22_FIXTURE_MODE": mode, "BRIDGEVM_PREBUILT_PROBE": "/forbidden/override", "CARGO_TARGET_DIR": "/forbidden/target", "BASH_ENV": str(shell_override)}))
            if more: more(stack)
            return preparer.prepare(self.args)

    def receipt(self): return json.loads((self.output / "preparation-receipt.json").read_bytes())

    def test_real_owned_process_query_shutdown_and_manifest(self):
        self.assertEqual(self.run_preparer(), 0)
        receipt = self.receipt()
        self.assertTrue(receipt["preparation_complete"]); self.assertTrue(receipt["natural_shutdown_observed"])
        self.assertFalse(receipt["claim_eligible"]); self.assertFalse(receipt["criterion_pass"])
        self.assertFalse(receipt["future_tpm_independence_proven"]); self.assertFalse(receipt["vtpm_configured"])
        self.assertEqual(receipt["decrypted_ntfs_volume_count"], 2)
        self.assertNotEqual(receipt["output_hashes"]["vars"], self.originals["vars"])
        self.assertEqual({"image": file_hash(self.fixture.disk), "vars": file_hash(self.fixture.vars)}, self.originals)
        manifest = self.output / "t22-input-manifest.tsv"
        from native_snapshot_restore_inputs import parse_manifest, authenticate
        rows = parse_manifest(manifest.read_bytes(), COMMIT)
        authenticate(rows, Path(rows["binary"][0]))
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o500)
        self.assertFalse((self.root / "override-executed").exists())
        with self.assertRaises(FileExistsError): self.run_preparer()

    def test_encrypted_data_and_bad_stop_refuse_publication(self):
        for mode in ("encrypted", "bad-stop"):
            with self.subTest(mode=mode):
                self.output = self.root / mode; self.args.output = self.output
                self.assertEqual(self.run_preparer(mode), 1)
                receipt = self.receipt()
                self.assertFalse(receipt["preparation_complete"]); self.assertTrue(receipt["cleanup_complete"])
                self.assertFalse((self.output / "t22-input-manifest.tsv").exists())
                self.assertTrue((self.output / "live/image.raw").exists())

    def test_missing_agent_refuses_with_owned_child_reaped(self):
        clock = iter(range(0, 100000, 301))
        def extra(stack): stack.enter_context(patch("t22_pair_runtime.time.monotonic", side_effect=lambda: next(clock)))
        self.assertEqual(self.run_preparer("no-service", extra), 1)
        self.assertTrue(self.receipt()["cleanup_complete"])
        self.assertFalse((self.output / "t22-input-manifest.tsv").exists())

    def test_uncertain_spawn_retains_writable_media_and_no_input(self):
        real_popen = subprocess.Popen
        for error in (InterruptedError, KeyboardInterrupt):
            with self.subTest(error=error):
                self.output = self.root / error.__name__; self.args.output = self.output
                def attempted(command, **kwargs):
                    if kwargs.get("start_new_session") is True: raise error()
                    return real_popen(command, **kwargs)
                def extra(stack): stack.enter_context(patch("t22_pair_runtime.subprocess.Popen", side_effect=attempted))
                self.assertEqual(self.run_preparer(more=extra), 1)
                self.assertFalse(self.receipt()["cleanup_complete"])
                self.assertTrue(self.receipt()["cleanup_required"])
                self.assertTrue((self.output / "cleanup-required.env").exists())
                self.assertTrue((self.output / "live/image.raw").stat().st_mode & 0o200)
                self.assertFalse((self.output / "t22-input-manifest.tsv").exists())

    def test_source_mutation_after_natural_shutdown_refuses(self):
        from t22_pair_runtime import shutdown_observed
        def altered(status, log):
            answer = shutdown_observed(status, log)
            self.fixture.disk.chmod(0o600)
            with self.fixture.disk.open("r+b") as stream: stream.write(b"changed owned source")
            self.fixture.disk.chmod(0o400)
            return answer
        def extra(stack): stack.enter_context(patch("t22_pair_runtime.shutdown_observed", side_effect=altered))
        self.assertEqual(self.run_preparer(more=extra), 1)
        self.assertFalse(self.receipt()["source_integrity"])
        self.assertFalse((self.output / "t22-input-manifest.tsv").exists())

    def test_publication_checks_output_hash_and_directory_identity(self):
        self.assertEqual(self.run_preparer(), 0)
        self.output.chmod(0o700)
        rows, data, docs = admit(*self.fixture.args())
        receipt = self.receipt()
        clones = {k: self.output / "live" / f for k, f in (("image", "image.raw"), ("vars", "vars.fd"))}
        identity = directory_identity(self.output)
        clone = clones["vars"]; clone.chmod(0o600)
        with clone.open("r+b") as stream: stream.write(b"post-observation mutation")
        clone.chmod(0o400)
        with self.assertRaisesRegex(ValueError, "changed"): publish(rows, clones, self.output, identity, receipt, COMMIT)
        old = self.output.with_name("retained-original"); self.output.rename(old); self.output.mkdir(mode=0o700)
        with self.assertRaises(ValueError): publish(rows, clones, self.output, identity, receipt, COMMIT)
        with self.assertRaises(ValueError): record(self.output, identity, receipt)
        self.assertEqual(list(self.output.iterdir()), [])

    def test_foreign_hardlink_and_replaced_child_refuse_before_chmod(self):
        from t22_pair_output_lock import seal_tree
        self.output.mkdir(mode=0o700)
        foreign = self.root / "foreign.txt"; foreign.write_text("preserve foreign permissions")
        os.link(foreign, self.output / "alias")
        before = foreign.stat().st_mode
        with self.assertRaises(ValueError):
            with seal_tree(self.output, directory_identity(self.output)): pass
        self.assertEqual(foreign.stat().st_mode, before)
        (self.output / "alias").unlink()
        child = self.output / "owned.txt"; child.write_text("owned")
        real_fchmod = os.fchmod
        def swapped(fd, mode):
            saved = self.output.with_name("saved-output")
            self.output.rename(saved); self.output.mkdir(mode=0o700)
            (self.output / "foreign.txt").write_text("replacement")
            (self.output / "foreign.txt").chmod(0o600)
            return real_fchmod(fd, mode)
        with patch("t22_pair_output_lock.os.fchmod", side_effect=swapped), self.assertRaises(ValueError):
            with seal_tree(self.output, directory_identity(self.output)): pass
        self.assertEqual((self.output / "foreign.txt").stat().st_mode & 0o777, 0o600)


if __name__ == "__main__": unittest.main()
