#!/usr/bin/env python3
"""Observe the exact T17 helper's Accessibility trust through LaunchServices."""
from __future__ import annotations
import argparse
import importlib.util
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path
HERE = Path(__file__).resolve().parent
REPORT_KEYS = {
    "schema", "observation_only", "criterion_pass", "accessibility_trusted",
    "caller_identity", "scope",
}
IDENTITY_KEYS = {
    "schema", "pid", "ppid", "bundle_id", "bundle_path_sha256",
    "executable_name", "scope", "caller_status", "static_code_status",
    "signing_status", "code_identifier", "code_cdhash",
}
HEX = re.compile(r"^[0-9a-f]+$")
class PreflightError(ValueError):
    pass
def load_manifest_module():
    source = HERE / "windows-product-e2e-manifest.py"
    spec = importlib.util.spec_from_file_location("t17_manifest", source)
    if spec is None or spec.loader is None:
        raise PreflightError("cannot load the T17 manifest verifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
def validate_report(value: object) -> None:
    if not isinstance(value, dict) or set(value) != REPORT_KEYS:
        raise PreflightError("diagnostic report has an unexpected field set")
    expected = {
        "schema": "t17.accessibility-diagnostic.v1",
        "observation_only": True,
        "criterion_pass": False,
        "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution",
    }
    for key, wanted in expected.items():
        if value[key] != wanted:
            raise PreflightError(f"diagnostic report has invalid {key}")
    identity = value["caller_identity"]
    if not isinstance(identity, dict) or set(identity) != IDENTITY_KEYS:
        raise PreflightError("diagnostic caller identity has an unexpected field set")
    identity_expected = {
        "schema": "t17.caller-identity.v1",
        "bundle_id": "dev.bridgevm.product-e2e",
        "executable_name": "BridgeVMProductE2E",
        "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution",
        "caller_status": "0",
        "static_code_status": "0",
        "signing_status": "0",
        "code_identifier": "dev.bridgevm.product-e2e",
    }
    for key, wanted in identity_expected.items():
        if identity[key] != wanted:
            raise PreflightError(f"diagnostic caller identity has invalid {key}")
    if not identity["pid"].isdigit() or not identity["ppid"].isdigit():
        raise PreflightError("diagnostic caller process identity is malformed")
    for key, size in (("bundle_path_sha256", 64), ("code_cdhash", 40)):
        field = identity[key]
        if not isinstance(field, str) or len(field) != size or not HEX.fullmatch(field):
            raise PreflightError(f"diagnostic caller identity has invalid {key}")
    if value["accessibility_trusted"] is not True:
        raise PreflightError("LaunchServices helper is not Accessibility-trusted")
def read_report(path: Path) -> object:
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid():
        raise PreflightError("diagnostic output is not an owned regular file")
    if info.st_size == 0 or info.st_size > 32 * 1024:
        raise PreflightError("diagnostic output is outside the 32 KiB bound")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise PreflightError("diagnostic output is not valid UTF-8 JSON") from error
def verified_helper(manifest: Path) -> tuple[Path, Path, str] | None:
    module = load_manifest_module()
    try:
        _, assets = module.parse(manifest)
    except (OSError, UnicodeError, module.ManifestError):
        return None
    app, app_digest = assets["app_bundle"]
    helper, helper_digest = assets["product_helper"]
    helper_app = app / "Contents/Helpers/BridgeVMProductE2E.app"
    if not app.is_dir() or app.is_symlink() or not helper.is_file() or helper.is_symlink():
        return None
    if not helper_app.is_dir() or helper_app.is_symlink():
        raise PreflightError("T17 helper app is not a real directory")
    try:
        if module.tree_hash(app, allow_symlinks=True) != app_digest:
            raise PreflightError("T17 app bundle hash does not match its manifest")
        if module.file_hash(helper) != helper_digest:
            raise PreflightError("T17 helper hash does not match its manifest")
    except module.ManifestError as error:
        raise PreflightError(f"T17 app bundle is unsafe: {error}") from error
    return app, helper_app, app_digest
def launch_and_observe(helper_app: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="bridgevm-t17-ax.") as directory:
        root = Path(directory)
        report, errors = root / "report.json", root / "launchservices.log"
        command = [
            "/usr/bin/open", "-n", str(helper_app), "--stdout", str(report),
            "--stderr", str(errors), "--args", "--accessibility-diagnostic",
        ]
        try:
            completed = subprocess.run(command, stdin=subprocess.DEVNULL, timeout=15, check=False)
        except subprocess.TimeoutExpired as error:
            raise PreflightError("LaunchServices diagnostic launch timed out") from error
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline and (not report.exists() or report.stat().st_size == 0):
            time.sleep(0.05)
        if not report.exists() or report.stat().st_size == 0:
            detail = errors.read_text(encoding="utf-8", errors="replace")[:512] if errors.exists() else ""
            raise PreflightError(f"LaunchServices diagnostic produced no report (open={completed.returncode}): {detail}")
        value = read_report(report)
        validate_report(value)
def preflight(manifest: Path) -> bool:
    if sys.platform != "darwin":
        return False
    verified = verified_helper(manifest)
    if verified is None:
        return False
    app, helper_app, app_digest = verified
    launch_and_observe(helper_app)
    module = load_manifest_module()
    try:
        if module.tree_hash(app, allow_symlinks=True) != app_digest:
            raise PreflightError("T17 app bundle changed during the LaunchServices observation")
    except module.ManifestError as error:
        raise PreflightError(f"T17 app bundle became unsafe: {error}") from error
    return True
def self_test() -> None:
    identity = {
        "schema": "t17.caller-identity.v1", "pid": "42", "ppid": "1",
        "bundle_id": "dev.bridgevm.product-e2e", "bundle_path_sha256": "a" * 64,
        "executable_name": "BridgeVMProductE2E",
        "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution",
        "caller_status": "0", "static_code_status": "0", "signing_status": "0",
        "code_identifier": "dev.bridgevm.product-e2e", "code_cdhash": "b" * 40,
    }
    report = {
        "schema": "t17.accessibility-diagnostic.v1", "observation_only": True,
        "criterion_pass": False, "accessibility_trusted": True,
        "caller_identity": identity,
        "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution",
    }
    validate_report(report)
    mutations = [
        {**report, "accessibility_trusted": False},
        {**report, "criterion_pass": True},
        {**report, "caller_identity": {**identity, "bundle_id": "wrong"}},
        {**report, "extra": "refuse"},
    ]
    for mutation in mutations:
        try:
            validate_report(mutation)
        except PreflightError:
            continue
        raise AssertionError("invalid diagnostic report was accepted")
    queue = (HERE / "bridgevm-live").read_text(encoding="utf-8")
    dispatch = queue.index('app-ui-manifest-dispatch.sh" validate')
    ledger = queue.index('local ledger_entry="$JOB_LEDGER/$id"')
    assert dispatch < ledger
def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("PASS: T17 LaunchServices preflight contracts")
        return 0
    if args.manifest is None:
        parser.error("--manifest is required")
    try:
        observed = preflight(args.manifest)
    except (OSError, PreflightError) as error:
        print(f"T17 submission preflight blocked: {error}", file=sys.stderr)
        return 1
    message = "passed" if observed else "not applicable"
    print(f"T17 LaunchServices Accessibility preflight {message}", file=sys.stderr)
    return 0
if __name__ == "__main__":
    raise SystemExit(main())
