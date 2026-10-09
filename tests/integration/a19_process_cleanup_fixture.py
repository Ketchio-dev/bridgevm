"""Isolated host and lifecycle fixture for the production T22 runner."""
from contextlib import redirect_stderr
from importlib.util import module_from_spec, spec_from_file_location
import io
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupted_restore_receipt as receipt

spec = spec_from_file_location("process_t22", ROOT / "scripts/live-gates/run-a19-interrupted-restore-tier.py")
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


class ProcessRunnerFixture:
    def run_main(self, output: Path, operation, complete: bool = False, *, preparation=None, seals=None) -> int:
        app = output.parent / "fixture.app"
        helper = app / runner.RELATIONS["snapshot_helper"]
        helper.parent.mkdir(parents=True)
        helper.write_text("fixture-helper")
        helper.chmod(0o700)

        def prepare(_manifest, _binary, _commit, prepared, *, on_created=None):
            prepared.mkdir()
            if on_created is not None:
                on_created(prepared)
            (prepared / "disk.raw").write_bytes(b"fixture disk")
            (prepared / "vars.fd").write_bytes(b"fixture vars")
            public = {**dict.fromkeys(receipt.HASHES, "a" * 64),
                      "image_sha256": "b" * 64, "vars_sha256": "b" * 64}
            return public, {"sealed_app": str(app), "app_cli": "fixture-cli", "binary": "fixture-probe"}

        error = io.StringIO()
        actual_output = subprocess.check_output
        def host_output(args, **options):
            if args[0] in ("git", "sysctl"):
                return "c" * 40 if args[0] == "git" else "Mac16,9"
            return actual_output(args, **options)
        with patch.object(sys, "argv", ["runner", str(output), "cleanup-fixture", "manifest", "binary"]), \
                patch.object(runner.subprocess, "check_output", side_effect=host_output), \
                patch.object(runner.platform, "mac_ver", return_value=("26.7", (), "")), \
                patch.object(runner, "sealed_hashes", side_effect=seals, return_value={
                    "input_manifest_sha256": "a" * 64, "binary_hash": "a" * 64}), \
                patch.object(runner, "prepare", side_effect=preparation or prepare) as prepared, \
                patch.object(runner, "run_lifecycle", side_effect=operation) as lifecycle, \
                patch.object(runner, "reauthenticate"), \
                patch.object(runner, "shutdown_count", return_value=4 if complete else 0), \
                redirect_stderr(error):
            status = runner.main()
        self.last_error = error.getvalue()
        self.preparation_call, self.lifecycle_call = prepared, lifecycle
        return status
