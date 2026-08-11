#!/usr/bin/env python3
"""Write the private A3 title-campaign receipt without private media paths."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def host_receipt(job_id: str, commit: str) -> dict:
    return {
        "gate_id": "a3-d3d11-real-title-3run",
        "criterion": "A3",
        "tested_commit": commit,
        "commit": commit,
        "tier": "t6-a3-title",
        "job_id": job_id,
        "host_os": "macOS " + subprocess.check_output(
            ["sw_vers", "-productVersion"], text=True).strip(),
        "host_hardware": subprocess.check_output(
            ["sysctl", "-n", "hw.model"], text=True).strip(),
        "finished_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }


def refusal(out: Path, job_id: str, commit: str, reason: str) -> int:
    receipt = host_receipt(job_id, commit)
    attempts = [run for run in range(1, 4) if (out / f"run-{run}").is_dir()]
    evidence = [f"run-{run}/run.log" for run in attempts if (out / f"run-{run}" / "run.log").exists()]
    receipt.update(
        {
            "sample_count": 0,
            "passes": 0,
            "failures": max(1, len(attempts)),
            "run_count": len(attempts),
            "required_run_count": 3,
            "evidence_paths": evidence,
            "known_confounders": [reason],
            "outcome": reason,
            "pass": False,
        }
    )
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return 0


def summary_values(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    return dict(
        line.split("=", 1)
        for line in path.read_text(errors="replace").splitlines()
        if "=" in line
    )


def final_receipt(args: argparse.Namespace) -> int:
    verified = dict(line.split("\t", 1) for line in
        (args.out / "verified-inputs.tsv").read_text().splitlines())
    attempts = [run for run in range(1, 4) if (args.out / f"run-{run}").is_dir()]
    samples: list[int] = []
    p50s: list[float] = []
    evidence: list[str] = []
    passes = 0
    for run in attempts:
        summary = args.out / f"run-{run}" / "summary.txt"
        values = summary_values(summary)
        samples.append(int(values.get("samples") or 0))
        p50s.append(float(values.get("p50") or 0))
        passes += values.get("a3_d3d11_fps") == "pass"
        evidence.append(f"run-{run}/" + ("summary.txt" if summary.exists() else "run.log"))

    receipt = host_receipt(args.job_id, args.commit)
    receipt.update(
        {
            "started_at": args.started_at,
            "guest_image_sha256": verified["image"],
            "image_sha256": verified["image"],
            "vars_sha256": verified["vars"],
            "driver_store_hash": verified["viogpu_dir"],
            "title_sha256": verified["title"],
            "ppsspp_sha256": args.ppsspp_executable_hash,
            "ppsspp_payload_sha256": verified["ppsspp"],
            "ppsspp_executable_sha256": args.ppsspp_executable_hash,
            "dxvk_d3d11_sha256": verified["d3d11"],
            "dxvk_dxgi_sha256": verified["dxgi"],
            "virglrenderer_sha256": verified["virglrenderer"],
            "moltenvk_sha256": verified["moltenvk"],
            "binary_hash": args.binary_hash,
            "gate_asset_hash": args.gate_asset_hash,
            "input_manifest_sha256": args.input_manifest_hash,
            "run_count": len(attempts),
            "required_run_count": 3,
            "sample_count": sum(samples),
            "fps_samples": samples,
            "fps_p50": p50s,
            "threshold_fps": 30.0,
            "passes": passes,
            "failures": len(attempts) - passes,
            "outcome": "completed" if passes == 3 else "failed",
            "pass": passes == 3,
            "module_identity_verified": passes == 3,
            "evidence_paths": evidence,
            "known_confounders": ["FPS is derived from the title's guest "
                "sceDisplaySetFrameBuf intervals; host RESOURCE_FLUSH is not used."],
        }
    )
    (args.out / "receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return 0 if passes == 3 else 1


def self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory)
        keys = "image vars viogpu_dir title ppsspp d3d11 dxgi virglrenderer moltenvk binary"
        (out / "verified-inputs.tsv").write_text(
            "".join(f"{key}\t{'ab' * 32}\n" for key in keys.split()))
        for run, state in enumerate(("pass", "pass", "fail"), 1):
            path = out / f"run-{run}"
            path.mkdir()
            (path / "summary.txt").write_text(
                f"a3_d3d11_fps={state}\nsamples=400\np50=58.82\n")
        args = argparse.Namespace(out=out, job_id="self-test", commit="0" * 40,
            started_at="2026-01-01T00:00:00Z", binary_hash="cd" * 32,
            ppsspp_executable_hash="34" * 32, gate_asset_hash="ef" * 32,
            input_manifest_hash="12" * 32)
        assert final_receipt(args) == 1
        receipt = json.loads((out / "receipt.json").read_text())
        assert receipt["passes"] == 2 and receipt["pass"] is False
        assert receipt["ppsspp_sha256"] == receipt["ppsspp_executable_sha256"] == "34" * 32
        assert receipt["run_count"] == receipt["required_run_count"] == 3
        (out / "run-3" / "summary.txt").write_text(
            "a3_d3d11_fps=pass\nsamples=400\np50=58.82\n"
        )
        assert final_receipt(args) == 0
        receipt = json.loads((out / "receipt.json").read_text())
        assert receipt["passes"] == 3 and receipt["pass"] is True
        for run in (2, 3):
            __import__("shutil").rmtree(out / f"run-{run}")
        assert final_receipt(args) == 1
        receipt = json.loads((out / "receipt.json").read_text())
        assert receipt["run_count"] == 1 and receipt["required_run_count"] == 3
    print("PASS: A3 receipt self-test")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--reason")
    parser.add_argument("--started-at")
    parser.add_argument("--binary-hash")
    parser.add_argument("--gate-asset-hash")
    parser.add_argument("--ppsspp-executable-hash")
    parser.add_argument("--input-manifest-hash")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.reason:
        return refusal(args.out, args.job_id, args.commit, args.reason)
    required = "started_at binary_hash ppsspp_executable_hash gate_asset_hash input_manifest_hash"
    for name in required.split():
        if not getattr(args, name):
            parser.error(f"--{name.replace('_', '-')} is required for a final receipt")
    return final_receipt(args)


if __name__ == "__main__":
    raise SystemExit(main())
