#!/usr/bin/env python3
"""Write a fail-closed, flat receipt for the Windows 1.0 closure live tier."""

from __future__ import annotations

import argparse
import json
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


def values(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    return dict(
        (line.split("\t", 1) if "\t" in line else line.split("=", 1))
        for line in path.read_text(errors="replace").splitlines()
        if "\t" in line or "=" in line
    )


def host_receipt(args: argparse.Namespace) -> dict:
    return {
        "gate_id": "windows-1.0-closure",
        "criterion": "F1-F4",
        "tested_commit": args.commit,
        "commit": args.commit,
        "tier": "t7-windows-closure",
        "job_id": args.job_id,
        "host_os": f"macOS {platform.mac_ver()[0]}",
        "host_hardware": subprocess.check_output(
            ["sysctl", "-n", "hw.model"], text=True
        ).strip(),
        "finished_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }


def write_receipt(args: argparse.Namespace) -> int:
    receipt = host_receipt(args)
    verified = values(args.out / "verified-inputs.tsv")
    summary = values(args.out / "proof" / "summary.txt")
    retained = values(args.out / "prepared" / "retained.env")
    injection = values(args.out / "injection" / "target-stat.txt")

    f1 = summary.get("f1_driver_load") == "pass"
    f2 = summary.get("f2_resize") == "pass"
    f3 = summary.get("f3_window_verbs") == "pass"
    f4_value = summary.get("f4_glyph_observation", "blocked")
    f4 = f4_value.startswith("measured-")
    prepared = retained.get("retained") == "true"
    injected = injection.get("injector_boot_observed") == "true"
    agent_match = verified.get("agent") == values(args.out / "injection/guest-logs/bvagent-package.log").get("agent_sha256")
    passed = prepared and injected and agent_match and f1 and f2 and f3 and f4

    receipt.update(
        {
            "input_manifest_sha256": args.input_manifest_hash,
            "binary_hash": verified.get("binary", "absent"),
            "image_sha256": verified.get("image", "absent"),
            "vars_sha256": verified.get("vars", "absent"),
            "injector_sha256": verified.get("injector", "absent"),
            "agent_sha256": verified.get("agent", "absent"),
            "driver_store_hash": verified.get("viogpu_dir", "absent"),
            "virglrenderer_sha256": verified.get("virglrenderer", "absent"),
            "moltenvk_sha256": verified.get("moltenvk", "absent"),
            "prepared_image_sha256": retained.get("image_sha256", "absent"),
            "prepared_vars_sha256": retained.get("vars_sha256", "absent"),
            "injector_boot_observed": injected,
            "module_identity_verified": agent_match,
            "f1_driver_load": f1,
            "f2_resize": f2,
            "f3_window_verbs": f3,
            "f4_glyph_observation": f4_value,
            "active_scanout_capture": summary.get("active_scanout_capture") == "present",
            "sample_count": 1 if (args.out / "proof" / "run.log").exists() else 0,
            "passes": int(f1) + int(f2) + int(f3) + int(f4),
            "failures": 4 - (int(f1) + int(f2) + int(f3) + int(f4)),
            "outcome": "completed" if passed else (args.reason or "failed"),
            "pass": passed,
            "evidence_paths": [
                path
                for path in (
                    "injection/target-stat.txt",
                    "prepared/retained.env",
                    "proof/summary.txt",
                    "proof/run.log",
                    "proof/captures/f4-notepad-focused.ppm",
                    "proof/captures/f4-ocr.txt",
                )
                if (args.out / path).exists()
            ],
            "known_confounders": [
                "F4 OCR is an observation aid; the retained active-scanout PPM is the primary evidence."
            ],
        }
    )
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "receipt.json").write_text(
        json.dumps(receipt, indent=2, sort_keys=True) + "\n"
    )
    return 0 if passed else 1


def self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory)
        (out / "verified-inputs.tsv").write_text(
            "".join(f"{key}\t{'ab' * 32}\n" for key in (
                "image", "vars", "injector", "agent", "viogpu_dir",
                "virglrenderer", "moltenvk", "binary"))
        )
        (out / "injection").mkdir()
        (out / "injection" / "target-stat.txt").write_text(
            "injector_boot_observed=true\n"
        )
        (out / "injection" / "guest-logs").mkdir()
        (out / "injection" / "guest-logs" / "bvagent-package.log").write_text(
            f"agent_sha256={'ab' * 32}\n"
        )
        (out / "prepared").mkdir()
        (out / "prepared" / "retained.env").write_text(
            f"retained=true\nimage_sha256={'cd' * 32}\nvars_sha256={'ef' * 32}\n"
        )
        (out / "proof").mkdir()
        (out / "proof" / "run.log").write_text("live\n")
        (out / "proof" / "summary.txt").write_text(
            "f1_driver_load=pass\nf2_resize=pass\nf3_window_verbs=pass\n"
            "f4_glyph_observation=blocked\nactive_scanout_capture=absent\n"
        )
        args = argparse.Namespace(
            out=out, job_id="self-test", commit="0" * 40,
            input_manifest_hash="12" * 32, reason=None
        )
        assert write_receipt(args) == 1
        receipt = json.loads((out / "receipt.json").read_text())
        assert not receipt["pass"] and receipt["passes"] == 3 and receipt["binary_hash"] == "ab" * 32
        (out / "proof" / "summary.txt").write_text(
            "f1_driver_load=pass\nf2_resize=pass\nf3_window_verbs=pass\n"
            "f4_glyph_observation=measured-visible-text\nactive_scanout_capture=present\n"
        )
        assert write_receipt(args) == 0
        assert json.loads((out / "receipt.json").read_text())["pass"]
    print("PASS: Windows closure receipt self-test")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--input-manifest-hash", required=True)
    parser.add_argument("--reason")
    return write_receipt(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
