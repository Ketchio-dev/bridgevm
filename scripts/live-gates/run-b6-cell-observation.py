#!/usr/bin/env python3
"""Physical-Mac B6 cell observation. Successful collection is never gate closure."""
import argparse
import datetime
import json
import pathlib
import platform
import re
import subprocess
import sys

from b6_cell_inputs import clone_pair, file_hash, load_inputs, small_bytes, verify_inputs

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from b6_frame_times import NO_CLAIM, summarize

TIER = "d2-b6-cell-observation"


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def receipt(job_id, commit):
    return dict(tier=TIER, gate_id="b6-cell-observation", criterion="B6",
                job_id=job_id, commit=commit, started_at=now(), outcome="incomplete",
                valid=False, **{"pass": False}, **NO_CLAIM, run_count=0,
                required_run_count=27, failure_code="incomplete")


def write_json(path, value, replace=False):
    raw = json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    destination = pathlib.Path(path)
    temporary = destination.with_name(destination.name + ".pending") if replace else destination
    with temporary.open("x", encoding="utf-8") as stream:
        stream.write(raw)
    if replace:
        temporary.replace(destination)


def validate_capture(out, config):
    capture = pathlib.Path(out) / "capture"
    lines = small_bytes(capture / "summary.txt", 8192).decode("utf-8").splitlines()
    fields = dict(line.split("=", 1) for line in lines)
    if len(fields) != len(lines):
        raise ValueError("duplicate summary field")
    expected = dict(width=str(config["width"]), height=str(config["height"]),
                    logpixels=str(config["logpixels"]), f1_driver_load="pass",
                    f2_resize="pass", scale_set="pass", runs_complete="true")
    if any(fields.get(key) != value for key, value in expected.items()):
        raise ValueError("cell summary does not prove the requested observation")
    runs = json.loads(small_bytes(capture / "runs.json", 16384))
    if not isinstance(runs, list) or len(runs) != 3 or any(not isinstance(row, dict) for row in runs):
        raise ValueError("expected three paired scene observations")
    if any(type(row.get("run")) is not int for row in runs) or [row["run"] for row in runs] != [1, 2, 3]:
        raise ValueError("run identities are incomplete or reused")
    hashes = []
    for row in runs:
        for key in ("classic_dpi", "packaged_dpi", "classic_focus", "packaged_focus",
                    "presentmon_status", "packaged_presentmon_status"):
            if row.get(key) != "pass":
                raise ValueError("scene prerequisite failed: " + key)
        for key in ("classic_capture", "packaged_capture", "presentmon_csv", "packaged_presentmon_csv"):
            if row.get(key) != "present":
                raise ValueError("scene artifact absent: " + key)
        for scene in ("classic", "packaged"):
            stem = "presentmon-" + scene + "-run" + str(row["run"])
            report_path = capture / (stem + ".frame-times.json")
            report = json.loads(small_bytes(report_path, 65536))
            if report.get("valid") is not True or any(report.get(key) is not False for key in NO_CLAIM):
                raise ValueError("invalid or claim-bearing diagnostic")
            issued = report.get("host_input_commands")
            if type(issued) is not int or issued < 1:
                raise ValueError("no recorded scene input")
            digest = report.get("guest_reported_sha256", "")
            if not isinstance(digest, str) or not re.fullmatch("[0-9a-f]{64}", digest):
                raise ValueError("missing guest CSV identity")
            parsed = summarize(capture / "share" / (stem + ".csv"), digest)[0]
            if any(report.get(key) != value for key, value in parsed.items()):
                raise ValueError("diagnostic differs from authenticated CSV")
            hashes.append(file_hash(report_path))
    return dict(schema_version=1, **config, run_count=3, frame_report_sha256=hashes, **NO_CLAIM)


def run(args, receipt_factory=None, complete=None):
    if not re.fullmatch("[A-Za-z0-9][A-Za-z0-9._-]{0,127}", args.job_id):
        raise ValueError("noncanonical job id")
    if not args.out.is_absolute() or not args.out.is_dir() or args.out.is_symlink():
        raise ValueError("output must be an existing absolute job directory")
    commit = subprocess.check_output(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).strip()
    value = (receipt_factory or receipt)(args.job_id, commit)
    destination = args.out / "receipt.json"
    write_json(destination, value)
    stage = "input"
    status = 1
    try:
        value["input_manifest_sha256"] = file_hash(args.input_manifest)
        records, config = load_inputs(args.input_manifest, args.sealed_binary)
        names = dict(image="image_sha256", vars="vars_sha256", binary="binary_hash",
                     virglrenderer="virglrenderer_sha256", moltenvk="moltenvk_sha256",
                     viogpu_dir="driver_store_hash", presentmon="gate_asset_hash", config="config_sha256")
        value.update({names[key]: digest for key, (_, digest) in records.items()})
        value["workload_profile"] = "b6-%sx%s-%sdpi-observation" % (config["width"], config["height"], config["logpixels"])
        stage = "host"
        if platform.system() != "Darwin" or platform.machine() != "arm64":
            raise ValueError("physical Apple-silicon Mac required")
        value["host_model"] = subprocess.check_output(["sysctl", "-n", "hw.model"], text=True).strip()
        value["macos_version"] = subprocess.check_output(["sw_vers", "-productVersion"], text=True).strip()
        subprocess.run([str(REPO / "scripts/live-gates/verify-windows-closure-binary.sh"),
                        str(args.sealed_binary), str(records["virglrenderer"][0])], check=True)
        stage = "clone"
        work = pathlib.Path.home() / "BridgeVM/work" / ("b6-cell-" + args.job_id)
        disk, variables = clone_pair(records, work)
        common = ["--target", str(disk), "--vars", str(variables), "--binary", str(args.sealed_binary),
                  "--viogpu-dir", str(records["viogpu_dir"][0]), "--moltenvk", str(records["moltenvk"][0]),
                  "--width", str(config["width"]), "--height", str(config["height"]),
                  "--logpixels", str(config["logpixels"])]
        for stage, script, timeout in (("scale", "b6-cell-set-scale.sh", 3600),
                                       ("capture", "b6-cell-capture.sh", 6000)):
            target = args.out / stage
            if target.exists() or target.is_symlink():
                raise ValueError("refusing an existing stage directory")
            command = [str(REPO / "scripts" / script), "--out", str(target)] + common
            if stage == "capture":
                command += ["--presentmon", str(records["presentmon"][0])]
            with (args.out / (stage + ".log")).open("x") as log:
                subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=timeout)
        stage = "evidence"
        result = validate_capture(args.out, config)
        result_path = args.out / "cell-observation.json"
        write_json(result_path, result)
        stage = "source-integrity"
        verify_inputs(records, args.sealed_binary)
        value["final_disk_sha256"] = file_hash(disk)
        value["final_vars_sha256"] = file_hash(variables)
        disk.chmod(0o400)
        variables.chmod(0o400)
        value.update(valid=True, outcome="observed", failure_code="none", run_count=3,
                     result_sha256=file_hash(result_path),
                     evidence_paths=["cell-observation.json", "capture/summary.txt", "capture/runs.json"],
                     known_confounders=value.get("known_confounders", []) + ["Single cell, not the 27-run matrix",
                                       "No reviewed glyph masks or accepted performance baseline",
                                       "full clone integrity hash immediately precedes boot (warm cache)"])
        if complete is not None: stage = "diagnostic"; complete(value, args.out)
        status = 0
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        value.update(valid=False, outcome="failed", run_count=0, failure_code=stage + "-failed",
                     diagnostic_error=str(error))
    finally:
        value["finished_at"] = now()
        write_json(destination, value, replace=True)
    return status


def main(run_cell=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--input-manifest", type=pathlib.Path, required=True)
    parser.add_argument("--sealed-binary", type=pathlib.Path, required=True)
    args = parser.parse_args()
    try:
        return (run_cell or run)(args)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("B6 observation refused: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
