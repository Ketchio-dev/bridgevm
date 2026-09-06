#!/usr/bin/env python3
"""Private, fixed-order media diagnostic. Completion never passes a criterion."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("manifest", Path(__file__).with_name("windows-media-comparison-manifest.py"))
manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manifest)


def clone_media(image, variables, work, image_hash, vars_hash):
    disk, vars_copy = work / "disk.raw", work / "vars.fd"
    subprocess.run(["cp", "-c", str(image), str(disk)], check=True)
    subprocess.run(["cp", str(variables), str(vars_copy)], check=True)
    for source, clone, expected in ((image, disk, image_hash), (variables, vars_copy, vars_hash)):
        if source.stat().st_dev != clone.stat().st_dev or source.stat().st_ino == clone.stat().st_ino:
            raise ValueError("media must be distinct same-volume clones")
        clone.chmod(0o600)
        if manifest.seal(source) != expected or manifest.seal(clone) != expected:
            raise ValueError("clone hash mismatch")
    return disk, vars_copy


def proof_command(repo, out, disk, variables, assets):
    return ["bash", str(repo / "scripts/windows-1.0-closure-interact.sh"),
            "--out", str(out), "--target", str(disk), "--vars", str(variables),
            "--binary", assets["binary"]["path"], "--viogpu-dir", assets["viogpu_dir"]["path"],
            "--moltenvk", assets["moltenvk"]["path"], "--watchdog-ms", "3000000"]


def idle_media(paths):
    result = subprocess.run(["lsof", "-t", "--", *map(str, paths)], capture_output=True)
    return result.returncode == 1 and not result.stdout and not result.stderr


def receipt(job_id, commit, digest, lanes, complete):
    return {"tier": "d1-windows-media-comparison", "job_id": job_id, "commit": commit,
            "input_manifest_sha256": digest, "claim_eligible": False, "criterion_pass": False,
            "capability_promotion": False, "pass": False, "passes": 0,
            "sample_count": len(lanes), "required_run_count": 2,
            "outcome": "diagnostic-complete" if complete else "diagnostic-incomplete",
            "known_confounders": ["Fixed original-then-reinjected order; two diagnostic observations only."],
            "lanes": lanes}


def run(args):
    args.out.mkdir(parents=True, exist_ok=False)
    lanes, complete = [], False
    digest = manifest.seal(args.input_manifest)
    commit = subprocess.check_output(["git", "-C", str(REPO), "rev-parse", "HEAD"], text=True).strip()
    try:
        checked = manifest.verify(args.input_manifest, REPO, args.sealed_binary)
        assets = checked["assets"]
        subprocess.run(["bash", str(REPO / "scripts/live-gates/verify-windows-closure-binary.sh"),
                        assets["binary"]["path"], assets["virglrenderer"]["path"]], check=True)
        environment = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM_")}
        environment["BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS"] = "100"
        for label in ("original", "reinjected"):
            if manifest.seal(args.input_manifest) != digest:
                raise ValueError("manifest changed between lanes")
            manifest.verify(args.input_manifest, REPO, args.sealed_binary)
            work = Path(tempfile.mkdtemp(prefix=f"{label}-", dir=args.out))
            disk, variables = clone_media(Path(assets[label]["path"]), Path(assets["vars"]["path"]), work, assets[label]["sha256"], assets["vars"]["sha256"])
            output = args.out / label
            with (args.out / f"{label}-launcher.log").open("wb") as log:
                result = subprocess.run(proof_command(REPO, output, disk, variables, assets),
                                        env=environment, stdout=log, stderr=subprocess.STDOUT)
            if not idle_media((disk, variables)):
                raise ValueError("lane media still in use; refusing next lane and retaining clones")
            manifest.verify(args.input_manifest, REPO, args.sealed_binary)
            if manifest.seal(args.input_manifest) != digest:
                raise ValueError("manifest changed during lane")
            lanes.append({"label": label, "image_sha256": assets[label]["sha256"],
                          "vars_sha256": assets["vars"]["sha256"], "exit_code": result.returncode,
                          "source_integrity_verified": True, "evidence_directory": label})
            (args.out / f"{label}-result.json").write_text(json.dumps(lanes[-1], indent=2) + "\n")
            shutil.rmtree(work)
        complete = True
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        (args.out / "failure.txt").write_text(str(error) + "\n")
    finally:
        (args.out / "receipt.json").write_text(json.dumps(receipt(args.job_id, commit, digest, lanes, complete), indent=2) + "\n")
    return 0 if complete else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--input-manifest", type=Path, required=True)
    parser.add_argument("--sealed-binary", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    raise SystemExit(run(parser.parse_args()))
