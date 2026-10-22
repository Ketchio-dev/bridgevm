#!/usr/bin/env python3
"""T19 tier fixtures: the tier itself accounts for each lane's imported vTPM key, whatever its helper did.

--stub-app APP and --source-package APP FOLDER give the live-tier smoke a file-keychain app and a source recovery
package made by its export. --augment-helper HELPER makes the synthetic import helper install the source's key
under its lane slug, as the product's import does, and forget it as its own cleanup would; key-left instead leaves
it plus one under an unpublished `windows-11` directory, key-crash leaves it and dies without a result,
key-refused leaves one the product refuses to forget, and key-cancel leaves it and cancels the release campaign
before lane 2. --tier-fixtures TIER MANIFEST TMP runs those jobs and request-tamper through the tier and reports
every case that fails.
"""
from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("t19_lane_key_forget_test", ROOT / "tests/integration/t19-lane-key-forget-test.py")
UNIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(UNIT)
AUGMENT = '''
keychain = pathlib.Path(request["app_bundle_path"]).parent / "fake-vtpm-keychain"; job = request["job_id"]; print("vm_slug=" + request["vm_slug"])
sealed = bytes.fromhex(json.loads(pathlib.Path(request["source_vtpm_package_path"]).read_text())["sealed"])
code = pathlib.Path(request["source_vtpm_code_path"]).read_text().strip().encode()
key = bytes(a ^ b for a, b in zip(sealed, hashlib.sha256(code).digest())).hex()
names = [request["vm_slug"]] + (["windows-11"] if "key-left" in job else [])
"key-left" in job and (library / "windows-11").mkdir()
for name in names: (keychain / (name + ".key")).write_text(key)
"key-refused" in job and (keychain / (request["vm_slug"] + ".refuse")).write_text("")
"key-cancel" in job and (pathlib.Path(request["work_parent"]).parent / "cancel.requested").write_text("")
if not any(case in job for case in ("key-left", "key-crash", "key-refused", "key-cancel")): (keychain / (request["vm_slug"] + ".key")).unlink()
if "key-crash" in job: pathlib.Path(args.result).unlink(); raise SystemExit(1)
'''
CASES = (  # job, outcome, failure code, worker cleanup verified
    ("key-pass-fixture", "completed", "none", True),  # the helper forgot its key; the product shows it absent
    ("key-left-fixture", "completed", "none", True),  # the helper left two keys; the tier forgets both
    ("key-crash-fixture", "failed", "product-model-failed", True),  # the helper died after its import
    ("key-cancel-fixture", "canceled", "canceled", True),  # the release campaign stopped between lanes
    ("key-refused-fixture", "cleanup-failed", "cleanup-failed", False),  # the product will not forget the key
    ("request-tamper-fixture", "cleanup-failed", "cleanup-failed", False),  # the rewritten request no longer names the lane
)


def tier_case(tier: Path, manifest: Path, temporary: Path, job: str, outcome: str, failure: str, verified: bool) -> list[str]:
    keychain, out, release = temporary / "fake-vtpm-keychain", temporary / f"{job}-out", temporary / "release.tsv"
    if "cancel" in job:
        release.write_text(manifest.read_text().replace("campaign_mode\tpilot\n", "campaign_mode\trelease\n", 1))
        manifest = release
    completed = subprocess.run([str(tier), "--out", str(out), "--input-manifest", str(manifest), "--job-id", job],
                               capture_output=True, text=True, check=False)
    private, receipt = out / "private", json.loads((out / "receipt.json").read_text())
    log = dict(line.split("=", 1) for line in (private / "lane-1-helper.log").read_text().splitlines() if "=" in line)
    lane, slug = Path(log["lane_root"]), log["vm_slug"]
    names = [] if "tamper" in job else [slug, "windows-11"] if "key-left" in job else [slug]
    inputs = lane / "inputs"
    calls = [call for call in UNIT.forget_calls(keychain) if call["--state-dir"].startswith(str(lane) + "/")]
    held = [name for name in names or [slug] if (keychain / f"{name}.key").exists()]
    failures: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            failures.append(f"{job}: {message}")
    check((receipt["outcome"], receipt["failure_code"], receipt["worker_cleanup_verified"], receipt["pass"])
          == (outcome, failure, verified, outcome == "completed"),
          f"receipt is {receipt['outcome']}/{receipt['failure_code']} with worker_cleanup_verified={receipt['worker_cleanup_verified']}")
    check([call["--stable-vm-id"] for call in calls] == names,
          f"the tier made {len(calls)} forget-import call(s) for the lane's {len(names)} candidate(s)")
    check(all((call["--state-dir"], call["--package"], call["--recovery-code-file"]) == (str(inputs / "vtpm"),
              str(inputs / "vtpm-recovery.json"), str(inputs / "vtpm-recovery-code.txt")) for call in calls),
          "a forget-import call did not use the lane's sealed inputs")
    if verified:
        check(not held, f"worker_cleanup_verified is true while {len(held)} lane key(s) remain")
        check(not os.path.lexists(lane.parent), "a verified cleanup kept the tree")
    else:
        check((private / "cleanup-failed").is_file() and (lane / "request.json").is_file(), "the refused cleanup did not keep the tree")
        check("T19 lane vTPM key forget refused" in completed.stderr, "the tier did not report the refusal")
        check("key-refused" not in job or held == [slug], "the key the product refused to forget is gone")
    check("tamper" not in job or ((receipt["run_count"], receipt["passes"], receipt["failures"]) == (1, 0, 1)
                                  and not (private / "lane-1-authenticated.json").exists()), "the rewritten request was authenticated")
    return failures


def run_tier_fixtures(tier: Path, manifest: Path, temporary: Path) -> int:
    failures = {job: tier_case(tier, manifest, temporary, job, *expected) for job, *expected in CASES}
    keychain = temporary / "fake-vtpm-keychain"
    source_touched = not (keychain / f"{UNIT.SOURCE_VM}.key").is_file() or any(
        call["--stable-vm-id"] == UNIT.SOURCE_VM for call in UNIT.forget_calls(keychain))
    for message in [message for messages in failures.values() for message in messages] + ["the source VM's key was touched"] * source_touched:
        print(f"FAIL: {message}", file=sys.stderr)
    failed = sum(bool(messages) for messages in failures.values())
    print(f"T19 tier key cases: {len(CASES) - failed} of {len(CASES)} passed")
    return 1 if failed or source_touched else 0


if __name__ == "__main__":
    command, arguments = sys.argv[1:2], [Path(argument) for argument in sys.argv[2:]]
    if command == ["--stub-app"]:
        UNIT.install_stub_app(*arguments)
    elif command == ["--source-package"]:
        UNIT.export_source(*arguments)
    elif command == ["--augment-helper"]:
        arguments[0].write_text(arguments[0].read_text() + AUGMENT)
    elif command == ["--tier-fixtures"]:
        raise SystemExit(run_tier_fixtures(*arguments))
    else:
        raise SystemExit("usage: --stub-app APP | --source-package APP FOLDER | --augment-helper HELPER | --tier-fixtures TIER MANIFEST TMP")
