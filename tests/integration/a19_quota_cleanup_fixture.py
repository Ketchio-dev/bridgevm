"""Synthetic T21 runner fixture: no queue submission, private media or cloning."""
from contextlib import ExitStack, redirect_stderr
from functools import partial
from importlib.util import module_from_spec, spec_from_file_location
import io
import json
from pathlib import Path
import shutil
from unittest.mock import patch

import a19_quota_refusal_receipt as receipt
import native_snapshot_restore_inputs as inputs

ROOT = Path(__file__).resolve().parents[2]
spec = spec_from_file_location("quota_cleanup_runner", ROOT / "scripts/live-gates/run-a19-quota-refusal-tier.py")
runner = module_from_spec(spec)
spec.loader.exec_module(runner)


def run_main(output, manifest, binary, *, clone_file=shutil.copyfile,
             clone_tree=shutil.copytree, **overrides):
    seals = {"input_manifest_sha256": inputs.digest(manifest), "binary_hash": inputs.digest(binary)}
    prepare = partial(inputs.prepare, clone_file=clone_file, clone_tree=clone_tree)
    errors = io.StringIO()
    with ExitStack() as stack:
        stack.enter_context(redirect_stderr(errors))
        stack.enter_context(patch.object(runner.sys, "argv", ["t21", str(output), "quota-fixture", str(manifest), str(binary)]))
        stack.enter_context(patch.object(runner, "sealed_hashes", return_value=seals))
        stack.enter_context(patch.object(runner, "prepare", side_effect=prepare))
        for name, operation in overrides.items():
            stack.enter_context(patch.object(runner, name, side_effect=operation))
        status = runner.main()
    value = receipt.validate(json.loads((output / "receipt.json").read_text()))
    return status, value, errors.getvalue()


def seal(output, manifest, binary, commit):
    """Write only this scratch fixture's seal; never invoke the queue CLI."""
    content = (f"job_id=quota-fixture\ntier={receipt.TIER}\ncommit={commit}\n"
               f"input_manifest_sha256={inputs.digest(manifest)}\nsealed_binary_sha256={inputs.digest(binary)}\n")
    (output / "job.env").write_text(content)
    ledger = output.parent.parent / "job-ledger/quota-fixture/entry.env"
    ledger.parent.mkdir(parents=True)
    ledger.write_text(content)
    ledger.chmod(0o400)
    shutil.copyfile(manifest, output / "input-manifest.tsv")
    shutil.copyfile(binary, output / "hvf_gic_boot_probe")


def helper_cases(source: str) -> str:
    """Inject actual helper failures before its normal create/verify handling."""
    return source.replace('if verb == "create":', '''if verb == "verify" and mode == "verify-failure":
    sys.exit(1)
if verb == "create":
    root = pathlib.Path(args[2])
    pair = pathlib.Path(args[0]).stat().st_size + pathlib.Path(args[1]).stat().st_size
    denied = pair > int(args[4])
    if (mode == "refusal-residue" and denied) or (mode == "boundary-partial" and not denied):
        for path in (root, root.with_name("." + root.name + ".staging"), root.parent / ".bridgevm-snapshot-parent-lease"):
            path.mkdir()
            (path / "partial").write_bytes(b"owned partial")
        if denied:
            print(f"snapshot would write {pair} bytes, over the {args[4]} byte quota", file=sys.stderr)
        sys.exit(1)
    if mode == "late-loose-output" and denied:
        for name in ("quota-refusal.snapshot", ".quota-refusal.snapshot.staging", "quota-boundary.snapshot", ".quota-boundary.snapshot.staging", ".bridgevm-snapshot-parent-lease"):
            path = root.parent.parent / name
            path.mkdir()
            (path / "sentinel").write_bytes(b"foreign late data")
    if mode == "mutate-app" and not denied:
        app_cli = root.parent / "BridgeVM.app/Contents/Resources/target/release/bridgevm"
        app_cli.write_bytes(b"mutated app")
if verb == "create":''').replace(
        '(root / "manifest.json").write_text(json.dumps(manifest))',
        '''if mode == "wrong-manifest": manifest["vm_id"] = "wrong-vm"
    (root / "manifest.json").write_text(json.dumps(manifest))''')
