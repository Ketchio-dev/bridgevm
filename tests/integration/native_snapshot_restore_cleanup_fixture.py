"""Tiny real T20 preparation; no private media, guest execution or queue access."""
from contextlib import redirect_stderr
from importlib.util import module_from_spec, spec_from_file_location
import io
import json
from pathlib import Path
import shutil
import sys
from types import SimpleNamespace
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import native_snapshot_restore_inputs as inputs
from native_snapshot_restore_receipt import validate

SCRIPT = ROOT / "scripts/live-gates/run-native-snapshot-restore-tier.py"
spec = spec_from_file_location("cleanup_t20", SCRIPT)
runner = module_from_spec(spec)
spec.loader.exec_module(runner)
COMMIT = "c" * 40


class CleanupFixture:
    def run_main(self, output, operation=None, *, clone_file=shutil.copyfile,
                 clone_tree=shutil.copytree, seal_error=None, bad_manifest=False):
        root = output.parent
        app = root / "source.app"
        artifacts = {"app_bundle": app, "image": root / "source.raw", "vars": root / "source.fd"}
        artifacts.update({key: app / relative for key, relative in inputs.RELATIONS.items()})
        for key, path in artifacts.items():
            if key != "app_bundle":
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(key.encode())
                path.chmod(0o700)
        manifest = root / "manifest.tsv"
        rows = [f"{key}\t{path}\t{inputs.tree_hash(path) if key == 'app_bundle' else inputs.digest(path)}"
                for key, path in artifacts.items()]
        rows += [f"source_commit\t{COMMIT}", "app_profile\trelease", "binary_profile\trelease",
                 "binary_features\tvenus", "rust_toolchain\t1.97.0"]
        manifest.write_text("invalid\n" if bad_manifest else "\n".join(rows) + "\n")
        seals = {"input_manifest_sha256": inputs.digest(manifest),
                 "binary_hash": inputs.digest(artifacts["binary"])}

        def prepare(manifest, binary, commit, directory, **options):
            return inputs.prepare(manifest, binary, commit, directory, clone_file, clone_tree, **options)

        actual_output, actual_run = runner.subprocess.check_output, runner.subprocess.run
        def host_output(args, **options):
            if args[0] in ("git", "sysctl"):
                return COMMIT if args[0] == "git" else "Mac16,9"
            return actual_output(args, **options)

        def lifecycle(args, **options):
            if args[0] != str(ROOT / "scripts/verify-native-snapshot-restore-boots.sh"):
                return actual_run(args, **options)
            self.lifecycle_call()
            return SimpleNamespace(returncode=1 if operation is None else operation())

        error = io.StringIO()
        self.lifecycle_call = Mock()
        with (patch.object(sys, "argv", ["runner", str(output), output.name, str(manifest), str(artifacts["binary"])]),
              patch.object(runner.subprocess, "check_output", side_effect=host_output),
              patch.object(runner.platform, "mac_ver", return_value=("26.7", (), "")),
              patch.object(runner, "sealed_hashes", return_value=seals, side_effect=seal_error),
              patch.object(runner, "prepare", side_effect=prepare) as preparation,
              patch.object(runner.subprocess, "run", side_effect=lifecycle),
              redirect_stderr(error)):
            try:
                return runner.main()
            finally:
                self.last_error = error.getvalue()
                self.preparation_call = preparation

    def failed_receipt(self, output, cleaned):
        value = validate(json.loads((output / "receipt.json").read_text()))
        self.assertFalse(value["pass"])
        self.assertEqual(value["worker_cleanup_verified"], cleaned, self.last_error)
        self.assertEqual((value["sample_count"], value["run_count"]), (0, 0))
        return value
