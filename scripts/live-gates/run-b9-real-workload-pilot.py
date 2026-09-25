#!/usr/bin/env python3
"""One physical-Mac VLC/AV1 diagnostic; never a B9 matrix criterion pass."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import shutil
import signal
import subprocess
import sys
import time

from b9_real_workload_inputs import (KEYS, REPO, clone_pair, driver_umd_hash, load_inputs,
                                     stage_external_pair, stage_share, stable_file,
                                     verify_inputs)
from b9_real_workload_observation import (collector_identity, observe,
                                          read_guest_json, ready_identity)
from b9_real_workload_receipt import job_fields
from b6_renderer_runtime import verify_renderer_runtime
from guest_input_controller import Controller
from guest_input_live_cleanup import stop

TIER = "d9-b9-real-workload"
JOB = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
NO_CLAIM = {"pass": False, "claim_eligible": False, "criterion_pass": False,
            "capability_promotion": False}


def sha(path: Path) -> str:
    return stable_file(path)[1]


def write_json(path: Path, value: dict) -> None:
    raw = (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(raw)


def lines(path: Path) -> list[str]:
    try:
        if path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError("guest run log exceeds observation bound")
        return path.read_bytes().decode("utf-8", errors="replace").replace("\r", "\n").splitlines()
    except FileNotFoundError:
        return []


def wait_for(check, process: subprocess.Popen, seconds: int, stage: str):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        if process.poll() is not None:
            raise ValueError(stage + ": VM exited before observation")
        time.sleep(0.2)
    raise TimeoutError(stage + ": bounded observation timed out")


def marker(share: Path, name: str, nonce: str) -> tuple[int, str]:
    target = share / name
    fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write((nonce + "\n").encode("ascii"))
    return stable_file(target, maximum=128)


def guest_share_bytes(log: Path, name: str) -> int | None:
    prefix = f"BVAGENT SHARE guest->host {name} bytes="
    matches = [line[len(prefix):] for line in lines(log) if line.startswith(prefix)]
    if not matches:
        return None
    if len(matches) != 1 or not re.fullmatch(r"[0-9]+ t=[0-9]+", matches[0]):
        raise ValueError("ambiguous or malformed guest share completion")
    return int(matches[0].split(" ", 1)[0])


def await_guest_file(share: Path, name: str, log: Path, process: subprocess.Popen,
                     seconds: int) -> tuple[dict, str]:
    target = share / name
    def complete():
        logged = guest_share_bytes(log, name)
        if logged is None:
            return None
        size, _ = stable_file(target, maximum=8192)
        if size != logged:
            raise ValueError("guest JSON differs from completed share byte count")
        return read_guest_json(target)
    return wait_for(complete, process, seconds, name)


def await_share_sync(log: Path, staged: dict, process: subprocess.Popen) -> None:
    def complete():
        current = lines(log)
        return all(any(line.startswith(f"BVAGENT SHARE host->guest {name} bytes={size} ")
                       for line in current) for name, (size, _) in staged.items())
    wait_for(complete, process, 900, "guest file share")


def await_firstboot(controller: Controller, process: subprocess.Popen) -> None:
    script = (r"$stage=Test-Path C:\BridgeVM\stage3.flag; "
              r"schtasks.exe /Query /TN BridgeVM-VioGpu3DFirstBoot *> $null; "
              r"if($stage -and $LASTEXITCODE -ne 0){Write-Output B9-FIRSTBOOT-READY}"
              r"else{Write-Output B9-FIRSTBOOT-PENDING}")
    command = 'powershell.exe -NoProfile -Command "' + script + '"'
    deadline = time.monotonic() + 1500
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise ValueError("VM exited before 3D firstboot readiness")
        controller.deadline = time.monotonic() + 30
        response = controller.send(command, command,
                                   ("B9-FIRSTBOOT-READY", "B9-FIRSTBOOT-PENDING"))
        if response == "B9-FIRSTBOOT-READY":
            return
        time.sleep(5)
    raise TimeoutError("3D firstboot readiness not observed")


def focus_window(controller: Controller, log: Path, hwnd: int, process: subprocess.Popen) -> None:
    command = f"WINFOCUS {hwnd}"
    before = len(lines(log))
    controller.write_command(command)
    wait_for(lambda: any(line == f"BVAGENT WINFOCUS {hwnd} -> OK WINFOCUS"
                         for line in lines(log)[before:]), process, 30, "VLC focus")
    script = ("Add-Type -Name Fg -Namespace B9 -MemberDefinition "
              "'[DllImport(\"user32.dll\")] public static extern System.IntPtr GetForegroundWindow();'; "
              "Write-Output ('B9-FOREGROUND-' + [B9.Fg]::GetForegroundWindow().ToInt64())")
    query = ('powershell.exe -NoProfile -EncodedCommand '
             + base64.b64encode(script.encode("utf-16le")).decode("ascii"))
    controller.deadline = time.monotonic() + 30
    controller.send(query, query, f"B9-FOREGROUND-{hwnd}")


def capture_frames(boot: Path, raw: Path, process: subprocess.Popen) -> list[Path]:
    iosurface = boot / "display.fb.iosurface"
    frames = []
    start = time.monotonic()
    for index, offset in enumerate((0, 2, 4, 6, 8)):
        while time.monotonic() - start < offset:
            if process.poll() is not None:
                break
            time.sleep(0.1)
        target = raw / f"scanout-{index}"
        attempt = subprocess.run([
            sys.executable, str(REPO / "scripts/capture-active-iosurface.py"),
            "--iosurface", str(iosurface), "--out", str(target), "--timeout-ms", "5000"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10, check=False)
        frame = target / "presented.ppm"
        if attempt.returncode == 0 and frame.is_file():
            frames.append(frame)
    return frames


def copy_raw(work: Path, raw: Path, share: Path, boot: Path, nonce: str) -> dict:
    raw.mkdir(mode=0o700, exist_ok=False)
    selected = ((boot / "run.log", "run.log"), (work / "launcher.log", "launcher.log"),
                (boot / "virtio-gpu.jsonl", "virtio-gpu.jsonl"),
                (share / ("ready-" + nonce + ".json"), "guest-ready.json"),
                (share / ("collector-" + nonce + ".json"), "guest-collector.json"),
                (share / ("finished-" + nonce + ".json"), "guest-finished.json"),
                (share / ("b9-" + nonce + ".csv"), "presentmon.csv"))
    result = {}
    for source, name in selected:
        if not source.is_file() or source.is_symlink() or source.stat().st_size == 0:
            continue
        target = raw / name
        with source.open("rb") as incoming, target.open("xb") as outgoing:
            shutil.copyfileobj(incoming, outgoing)
        target.chmod(0o600)
        result[name] = {"bytes": target.stat().st_size, "sha256": sha(target)}
    return result


def diagnostic_stop(request: Path, process: subprocess.Popen) -> None:
    if process.poll() is not None or request.exists():
        return
    fd = os.open(request, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(b"")  # The host parser's supported legacy D3 request is empty.
    try:
        process.wait(timeout=35)
    except subprocess.TimeoutExpired:
        pass


def run(args) -> int:
    if not JOB.fullmatch(args.job_id) or not args.out.is_absolute() or not args.out.is_dir():
        raise ValueError("B9 job identity or output directory is invalid")
    commit = subprocess.check_output(["git", "-C", str(REPO), "rev-parse", "HEAD"],
                                     text=True).strip()
    job = job_fields(args.out.parent.resolve())
    if job["commit"] != commit or job["job_id"] != args.job_id:
        raise ValueError("queue-sealed B9 source or job identity differs")
    receipt = {"schema": "bridgevm.b9-real-workload-pilot.v1", "tier": TIER,
               "criterion": "B9", "candidate_id": "vlc-3.0.23-arm64-kodi-bbb-1080p-av1-10s-v1",
               "job_id": args.job_id, "commit": commit,
               "input_manifest_sha256": sha(args.input_manifest.resolve()),
               "sealed_binary_sha256": sha(args.sealed_binary.resolve()),
               "outcome": "diagnostic-incomplete", "result_class": "GUEST_NOT_READY",
               "pilot_count": 0, "required_workload_count": 20,
               "cleanup_complete": False, "source_integrity": False,
               "frame_count": 0, "scanout_sample_count": 0,
               "owned_vm_pgid": 0,
               **NO_CLAIM}
    receipt["asset_hashes"] = {
        key: job["asset_" + key + "_sha256"] for key in KEYS}
    if (receipt["input_manifest_sha256"] != job["input_manifest_sha256"]
            or receipt["sealed_binary_sha256"] != job["sealed_binary_sha256"]):
        raise ValueError("queue-sealed B9 manifest or binary changed")
    stage, process, work, records, clones = "inputs", None, None, None, None
    nonce = secrets.token_hex(16)
    receipt["nonce_sha256"] = hashlib.sha256(nonce.encode()).hexdigest()
    raw = args.out / "raw"
    share = boot = None
    try:
        if platform.system() != "Darwin" or platform.machine() != "arm64":
            raise ValueError("physical Apple-silicon Mac required")
        records = load_inputs(args.input_manifest.resolve(), commit, args.sealed_binary.resolve())
        receipt["asset_hashes"] = {key: digest for key, (_, digest) in records.items()}
        umd_hash = driver_umd_hash(records)
        receipt["driver_umd_sha256"] = umd_hash
        verify_renderer_runtime(records)
        subprocess.run([str(REPO / "scripts/live-gates/verify-windows-closure-binary.sh"),
                        str(args.sealed_binary), str(records["virglrenderer"][0])],
                       check=True, timeout=60)
        subprocess.run(["codesign", "--verify", "--strict", str(args.sealed_binary)],
                       check=True, timeout=30)
        stage = "clone"
        parent = (Path.home() / "BridgeVM/work").resolve(strict=True)
        work = parent / ("b9-pilot-" + args.job_id)
        receipt["owned_work_path"] = str(work)
        work.mkdir(mode=0o700, exist_ok=False)
        staged = stage_external_pair(records, work / "staged-cache")
        disk, variables = clone_pair(staged, work / "lane")
        clones = {"image": disk, "vars": variables}
        share, boot = work / "share", work / "boot"
        staged_files = stage_share(records, share)
        receipt["staged_file_hashes"] = {name: digest for name, (_, digest) in staged_files.items()}
        boot.mkdir(mode=0o700, exist_ok=False)
        control, input_control = work / "agent.ctl", work / "input.ctl"
        for path in (control, input_control):
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            os.close(fd)
        stop_request = work / "diagnostic-stop.request"
        command = ["bash", str(REPO / "scripts/run-hvf-windows-installed-boot.sh"),
                   "--target", str(disk), "--vars", str(variables),
                   "--firmware-code", str(records["firmware"][0]),
                   "--evidence-dir", str(boot), "--release", "--skip-build",
                   "--watchdog-ms", "2700000", "--max-reboots", "8",
                   "--ram-mib", "6144", "--smp-cpus", "4", "--enable-xhci",
                   "--agent-service-control", str(control),
                   "--agent-share-host", str(share), "--agent-share-guest", r"C:\BridgeVMB9",
                   "--agent-share-ms", "500", "--agent-share-max-kb", "8192",
                   "--input-control", str(input_control), "--virtio-gpu-3d",
                   "--gpu-trace", str(boot / "virtio-gpu.jsonl"),
                   "--gpu-trace-protocol", "venus", "--viogpu3d-dir", str(records["viogpu_dir"][0]),
                   "--display-export-ppm", str(boot / "display-live.ppm"),
                   "--display-export-fb", str(boot / "display.fb"),
                   "--display-export-ms", "100"]
        env = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM")}
        env.pop("VREND_DEBUG", None)
        env.update(BRIDGEVM_PREBUILT_PROBE=str(args.sealed_binary),
                   BRIDGEVM_VULKAN_LIB=str(records["moltenvk"][0]),
                   BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT="1",
                   BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT="0",
                   BRIDGEVM_VIRTIO_GPU_ASYNC_PRESENT="0",
                   BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST=str(stop_request),
                   BRIDGEVM_BOOT_PROGRESS_KILL="1")
        stage = "boot"
        with (work / "launcher.log").open("xb") as launcher:
            process = subprocess.Popen(command, cwd=REPO, env=env, stdout=launcher,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            receipt["owned_vm_pgid"] = process.pid
            controller = Controller(control, boot / "run.log", share, timeout=120)
            wait_for(lambda: any(line.startswith("BVAGENT SERVICE start")
                                 for line in lines(boot / "run.log")), process, 1800, "guest service")
            await_share_sync(boot / "run.log", staged_files, process)
            await_firstboot(controller, process)
            stage = "vlc"
            guest_script = r"C:\BridgeVMB9\bv-b9-vlc-playback.ps1"
            script = ("$p='" + guest_script + "'; "
                      "if((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() "
                      "-cne '" + records["guest_script"][1] + "'){exit 42}; "
                      "$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create "
                      "-Arguments @{CommandLine='powershell.exe -NoProfile -ExecutionPolicy Bypass "
                      "-File " + guest_script + " -Nonce " + nonce
                      + " -ExpectedD3D11UmdSha " + umd_hash + "'}; "
                      "if($r.ReturnValue -ne 0 -or $r.ProcessId -le 0){exit 41}; "
                      "Write-Output 'B9-WORKLOAD-LAUNCHED-" + nonce + "'")
            launch = 'powershell.exe -NoProfile -Command "' + script + '"'
            controller.deadline = time.monotonic() + 60
            controller.send(launch, launch, "B9-WORKLOAD-LAUNCHED-" + nonce)
            ready, receipt["ready_sha256"] = await_guest_file(
                share, "ready-" + nonce + ".json", boot / "run.log", process, 300)
            pins = {"media_sha256": records["media"][1],
                    "vlc_zip_sha256": records["vlc_zip"][1],
                    "presentmon_sha256": records["presentmon"][1],
                    "expected_driver_umd_sha256": umd_hash}
            pid, hwnd = ready_identity(ready, nonce, pins)
            focus_window(controller, boot / "run.log", hwnd, process)
            size, _ = marker(share, "collector-go-" + nonce + ".txt", nonce)
            wait_for(lambda: any(line.startswith(
                f"BVAGENT SHARE host->guest collector-go-{nonce}.txt bytes={size} ")
                for line in lines(boot / "run.log")), process, 60, "collector start marker")
            collector, receipt["collector_sha256"] = await_guest_file(
                share, "collector-" + nonce + ".json", boot / "run.log", process, 60)
            collector_identity(collector, nonce, pid)
            focus_window(controller, boot / "run.log", hwnd, process)
            before = sum(line.startswith("live input accepted: command=Key(")
                         for line in lines(boot / "run.log"))
            with input_control.open("ab", buffering=0) as input_stream:
                input_stream.write(b"KEY space\n")
            wait_for(lambda: sum(line.startswith("live input accepted: command=Key(")
                                 for line in lines(boot / "run.log")) > before,
                     process, 30, "visible VLC play input")
            marker(share, "play-start-" + nonce + ".txt", nonce)
            stage = "playback"
            raw.mkdir(mode=0o700, exist_ok=False)
            frames = capture_frames(boot, raw, process)
            await_guest_file(share, "finished-" + nonce + ".json",
                             boot / "run.log", process, 90)
            csv_name = "b9-" + nonce + ".csv"
            def csv_complete():
                logged = guest_share_bytes(boot / "run.log", csv_name)
                if logged is None:
                    return None
                size, digest = stable_file(share / csv_name, maximum=7_500_000)
                if size != logged:
                    raise ValueError("PresentMon CSV differs from completed share byte count")
                return digest
            wait_for(csv_complete, process, 90, "PresentMon CSV share")
            observed = observe(share, nonce, pins, frames)
            receipt.update(observed)
            receipt["scanout_files"] = [frame.relative_to(raw).as_posix() for frame in frames]
            receipt["pilot_count"] = 1
            stage = "shutdown"
            controller.write_command("shutdown /s /f /t 0")
            receipt["guest_shutdown_exit"] = process.wait(timeout=120)
            receipt["guest_shutdown_observed"] = receipt["guest_shutdown_exit"] == 0 and any(
                line.startswith("stop: PSCI SYSTEM_OFF") for line in lines(boot / "run.log"))
            if not receipt["guest_shutdown_observed"]:
                raise ValueError("clean guest shutdown not observed")
            receipt["outcome"] = "diagnostic-complete"
    except (OSError, ValueError, TimeoutError, subprocess.SubprocessError) as error:
        receipt["failure_stage"] = stage
        receipt["failure_type"] = type(error).__name__
        if isinstance(error, ValueError) and stage in ("vlc", "playback"):
            receipt["result_class"] = "INVALID_EVIDENCE"
        elif stage == "vlc":
            receipt["result_class"] = ("COLLECTOR_FAILED" if receipt.get("ready_sha256")
                                       and not receipt.get("collector_sha256") else
                                       "PLAYBACK_INCOMPLETE" if receipt.get("collector_sha256")
                                       else "GUEST_NOT_READY")
        elif stage == "playback" or (stage == "shutdown" and
                                     receipt["result_class"] == "VISIBLE_PLAYBACK_COMPLETE"):
            receipt["result_class"] = "PLAYBACK_INCOMPLETE"
    finally:
        if process is not None:
            try:
                diagnostic_stop(work / "diagnostic-stop.request", process)
                receipt["owned_process_group_stopped"] = stop(process)
            except (OSError, ValueError, subprocess.SubprocessError):
                receipt["owned_process_group_stopped"] = False
        else:
            receipt["owned_process_group_stopped"] = True
        if work is not None and work.exists() and receipt["owned_process_group_stopped"]:
            try:
                if not raw.exists():
                    receipt["private_artifacts"] = copy_raw(work, raw, share, boot, nonce) if share else {}
                    receipt["private_artifact_dir"] = "raw"
                else:
                    receipt["private_artifacts"] = copy_raw_existing(work, raw, share, boot, nonce)
                    receipt["private_artifact_dir"] = "raw/guest"
                if records is not None:
                    verify_inputs(records, args.sealed_binary.resolve())
                    receipt["source_integrity"] = True
                if clones is not None:
                    receipt["clone_final_sha256"] = {key: sha(path) for key, path in clones.items()}
                shutil.rmtree(work)
                receipt["cleanup_complete"] = not work.exists()
            except (OSError, ValueError, subprocess.SubprocessError):
                receipt["cleanup_complete"] = False
        elif work is None:
            receipt["cleanup_complete"] = receipt["owned_process_group_stopped"]
        write_json(args.out / "receipt.json", receipt)
    return 1  # A diagnostic never returns a queue-level criterion pass.


def copy_raw_existing(work: Path, raw: Path, share: Path, boot: Path, nonce: str) -> dict:
    """Copy late guest artifacts after scanout captures created the raw directory."""
    target = raw / "guest"
    return copy_raw(work, target, share, boot, nonce)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--input-manifest", type=Path, required=True)
    parser.add_argument("--sealed-binary", type=Path, required=True)
    try:
        return run(parser.parse_args())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print("B9 pilot could not write a sealed private receipt: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
