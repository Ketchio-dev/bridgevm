#!/usr/bin/env python3
"""Live check of the guest microphone with the test WAV; never the Mac's input device.

    hda-microphone-live-check.py --probe PROBE --out DIR [--name NAME] [--load-max 12]

Boots one APFS clone of the perf canonical image (~/.bvm-perf-internal) with PROBE, the
HDA device and BRIDGEVM_HDA_CAPTURE_TEST_WAV: a 10 s s16le 48 kHz stereo WAV whose frame
i holds L = i & 0xFFFF and R = 0xA500 | (i >> 16). BRIDGEVM_HDA_MICROPHONE is never set,
so no Mac input device opens and macOS shows no privacy prompt. Through the agent and the
share it runs scripts/win-assets/bv-mic-capture.ps1, which lists Windows' capture
endpoints and captures the default one for 3 s in WASAPI exclusive mode, in the first format
IsFormatSupported accepts (48 kHz 16-bit stereo as WAVEFORMATEX, as WAVEFORMATEXTENSIBLE,
then the endpoint's device format), and then for 3 s in shared mode (GetMixFormat).

It starts after 60 s with no other VM (pgrep -x) and a 1-minute load under --load-max. A
signed sentinel named like the VMM runs only while the run does. NAME is one path
component; the run creates ~/.bvm-perf-internal/lanes/NAME itself, refusing one that
exists, and deletes only that lane, with the clones in it.

Pass, fixed before the first run:
  - Windows lists at least one capture endpoint, and every one has status OK;
  - the capture returns 3 s of frames;
  - the pattern starts within the first 0.5 s and then runs without a break;
  - its first 2 s hash as the same span of the WAV (bit-exact);
  - the host log shows the test WAV source and no Mac microphone line.
Also reported, outside the pass: each exclusive format Windows was asked about and its
HRESULT, and the shared-mode capture's format, correlation, gain and phase against the
pattern (the engine converts and may apply volume there, so it is not bit-exact).
Writes DIR/summary.json and DIR/run.log; exits 0 on a pass.
"""
import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import signal
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CANON = Path.home() / ".bvm-perf-internal"
GUEST_SHARE = r"C:\BridgeVM\mic"
VMMS = "hvf_gic_boot_probe|qemu-system-aarch64|tart|prl_vm_app"
RATE, PATTERN_FRAMES, CAPTURE_SECONDS, SPAN_FRAMES = 48_000, 480_000, 3, 96_000
START_LIMIT_FRAMES = RATE // 2


def log(message):
    print(time.strftime("%H:%M:%SZ", time.gmtime()), message, flush=True)


def pattern(first, frames):
    """The pattern's s16le stereo bytes for `frames` frames from index `first`, looped."""
    values = []
    for k in range(frames):
        index = (first + k) % PATTERN_FRAMES
        values += [index & 0xFFFF, 0xA500 | (index >> 16)]
    return struct.pack(f"<{len(values)}H", *values)


def pattern_wav():
    fmt = struct.pack("<HHIIHH", 1, 2, RATE, RATE * 4, 4, 16)
    data = pattern(0, PATTERN_FRAMES)
    body = b"WAVE" + b"fmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(data)) + data
    return b"RIFF" + struct.pack("<I", len(body)) + body


def other_vms(own=()):
    pids = subprocess.run(["pgrep", "-x", VMMS], capture_output=True, text=True).stdout.split()
    return [pid for pid in pids if pid not in own]


def wait_for_quiet_host(load_max, limit_s=12 * 3600):
    start, quiet = time.monotonic(), None
    while time.monotonic() - start < limit_s:
        if other_vms() or os.getloadavg()[0] >= load_max:
            quiet = None
        elif quiet is None:
            quiet = time.monotonic()
        elif time.monotonic() - quiet >= 60:
            return True
        time.sleep(5)
    return False


def lane_name(value):
    """A lane directory name: one path component, never '.', '..' or a path."""
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}", value):
        raise argparse.ArgumentTypeError(f"{value!r} is not one path component of letters, digits, '.', '_' or '-'")
    return value


def copy_executable(source, target):
    shutil.copyfile(source, target)
    os.chmod(target, 0o755)


class Guest:
    """The run.log reader and agent.ctl writer of one run."""

    def __init__(self, log_path, ctl, proc):
        self.log_path, self.ctl, self.proc = log_path, ctl, proc
        self.offset, self.pending = 0, ""

    def lines(self, deadline):
        while time.monotonic() < deadline:
            if self.log_path.exists():
                with open(self.log_path, "rb") as handle:
                    handle.seek(self.offset)
                    data = handle.read()
                self.offset += len(data)
                self.pending += data.decode("utf-8", "replace")
                *done, self.pending = self.pending.split("\n")
                yield from done
            if self.proc.poll() is not None:
                raise RuntimeError(f"runner exited {self.proc.returncode}")
            time.sleep(0.2)
        raise TimeoutError("guest deadline")

    def wait_for(self, prefix, timeout):
        for line in self.lines(time.monotonic() + timeout):
            if line.startswith(prefix):
                return line

    def command(self, command, timeout=90):
        with open(self.ctl, "a") as handle:
            handle.write(command + "\n")
        head, body, code = f"BVAGENT CMD {command} exit=", None, None
        try:
            for line in self.lines(time.monotonic() + timeout):
                if body is None and line.startswith(head):
                    body, code = [], int(line[len(head):])
                elif body is not None and line == f"BVAGENT END {command}":
                    return code, [entry.rstrip("\r") for entry in body]
                elif body is not None:
                    body.append(line)
        except TimeoutError:
            return "timeout", body or []


def parse_capture(body):
    endpoints, capture, error, formats, shared = [], None, None, [], None
    for line in body:
        if line.startswith("MIC ENDPOINT "):
            fields = dict(token.split("=", 1) for token in line[len("MIC ENDPOINT "):].split(" "))
            name = base64.b64decode(fields.get("name", "")).decode("utf-8", "replace")
            endpoints.append({"status": fields.get("status"), "name": name})
        elif line.startswith("MIC CAPTURE "):
            capture = dict(token.split("=", 1) for token in line[len("MIC CAPTURE "):].split(" "))
        elif line.startswith("MIC FORMAT "):
            formats.append(line[len("MIC FORMAT "):])
        elif line.startswith("MIC SHARED ERROR"):
            shared = {"error": line[len("MIC SHARED ERROR "):]}
        elif line.startswith("MIC SHARED "):
            shared = dict(token.split("=", 1) for token in line[len("MIC SHARED "):].split(" ") if "=" in token)
        elif line.startswith("MIC ERROR"):
            error = line
    return endpoints, capture, error, formats, shared


def verdict(endpoints, capture, host_lines):
    value = (lambda key: int((capture or {}).get(key, -1)))
    first = value("first_index")
    expected = hashlib.sha256(pattern(first, SPAN_FRAMES)).hexdigest() if first >= 0 else None
    checks = {
        "capture_endpoints_ok": bool(endpoints) and all(entry["status"] == "OK" for entry in endpoints),
        "three_seconds_captured": value("frames") >= CAPTURE_SECONDS * RATE,
        "pattern_starts_within_half_a_second": 0 <= value("first_offset") <= START_LIMIT_FRAMES,
        "no_break_after_the_start": capture is not None and value("breaks") == 0,
        "two_seconds_bit_exact": value("span_frames") == SPAN_FRAMES
        and (capture or {}).get("span_sha256") == expected,
        "host_used_the_test_wav": any(line.startswith("HDA capture: test WAV ") for line in host_lines),
        "mac_microphone_never_opened": not any(
            "Mac microphone" in line or "the Mac's default input" in line or "BRIDGEVM_HDA_MICROPHONE" in line
            for line in host_lines),
    }
    return checks, all(checks.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--name", type=lane_name, default=time.strftime("hda-mic-%Y%m%d-%H%M%S", time.gmtime()))
    parser.add_argument("--load-max", type=float, default=12.0)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    lane = CANON / "lanes" / args.name
    summary = {"name": args.name, "probe_sha256": hashlib.sha256(args.probe.read_bytes()).hexdigest(),
               "commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True,
                                        text=True).stdout.strip()}
    holder = proc = None
    created = False  # only a lane this run created is ever read from or deleted
    try:
        while True:
            log(f"waiting for 60 s with no other VM and load < {args.load_max}")
            if not wait_for_quiet_host(args.load_max):
                raise RuntimeError("the host was not quiet within 12 h")
            if not created:
                (CANON / "lanes").mkdir(exist_ok=True)
                lane.mkdir()  # exclusive: another run's lane is refused, never reused
                created = True
            sentinel = lane / "sentinel" / "hvf_gic_boot_probe"
            sentinel.parent.mkdir(exist_ok=True)
            copy_executable("/bin/sleep", sentinel)
            subprocess.run(["codesign", "--sign", "-", "--force", str(sentinel)], check=True, capture_output=True)
            holder = subprocess.Popen([str(sentinel), "3600"], start_new_session=True)
            time.sleep(2)
            if holder.poll() is not None:
                raise RuntimeError(f"the sentinel exited {holder.returncode} at launch")
            if not other_vms(own=[str(holder.pid)]):
                break
            log("another VM started with ours; waiting again")  # another waiter saw the same quiet minute
            holder.kill(), holder.wait()
            holder = None
        summary["start_load"] = round(os.getloadavg()[0], 2)
        disk, vars_fd, wav = lane / "disk.raw", lane / "vars.fd", lane / "capture-test.wav"
        subprocess.run(["cp", "-c", str(CANON / "canonical.raw"), str(disk)], check=True)
        subprocess.run(["cp", "-c", str(CANON / "canonical-vars.fd"), str(vars_fd)], check=True)
        os.chmod(disk, 0o600), os.chmod(vars_fd, 0o600)
        wav.write_bytes(pattern_wav())
        probe = lane / "bin" / "hvf_gic_boot_probe"
        probe.parent.mkdir(exist_ok=True)
        copy_executable(args.probe, probe)
        subprocess.run(["codesign", "--sign", "-", "--force", "--entitlements",
                        str(ROOT / "apps/macos/HvfRunner.entitlements"), str(probe)], check=True, capture_output=True)
        share = lane / "share"
        share.mkdir(exist_ok=True)
        for name in ("bv-mic-capture.ps1", "bv-mic-capture.cs"):
            shutil.copyfile(ROOT / "scripts/win-assets" / name, share / name)
        evidence, ctl = lane / "hvf", lane / "ctl.txt"
        ctl.write_text("")
        cmd = ["bash", str(ROOT / "scripts/run-hvf-windows-installed-boot.sh"), "--target", str(disk),
               "--vars", str(vars_fd), "--evidence-dir", str(evidence), "--release", "--skip-build",
               "--smp-cpus", "4", "--ram-mib", "6144", "--agent-service-control", str(ctl),
               "--watchdog-ms", "1800000", "--no-guest-disk-harvest", "--enable-xhci", "--hda",
               "--agent-share-host", str(share), "--agent-share-guest", GUEST_SHARE, "--agent-share-ms", "500",
               "--agent-share-max-kb", "8192"]
        env = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM")}
        env.update(BRIDGEVM_PREBUILT_PROBE=str(probe), BRIDGEVM_GUEST_RTC_EPOCH="1785542400",
                   BRIDGEVM_HDA_CAPTURE_TEST_WAV=str(wav))
        assert "BRIDGEVM_HDA_MICROPHONE" not in env and "--hda-microphone" not in cmd
        log(f"booting {args.name} (load {summary['start_load']})")
        with open(args.out / "runner.log", "w") as runner_log:
            proc = subprocess.Popen(cmd, cwd=ROOT, env=env, stdout=runner_log, stderr=subprocess.STDOUT,
                                    start_new_session=True)
        guest = Guest(evidence / "run.log", ctl, proc)
        started = time.monotonic()
        guest.wait_for("BVAGENT SERVICE start", 600)
        summary["agent_start_s"] = round(time.monotonic() - started, 1)
        staged, deadline = False, time.monotonic() + 240
        while not staged and time.monotonic() < deadline:
            code, body = guest.command(f"if exist {GUEST_SHARE}\\bv-mic-capture.cs (echo staged)")
            staged = code == 0 and "staged" in " ".join(body)
            if not staged:
                time.sleep(5)
        if not staged:
            raise RuntimeError("the share never reached the guest")
        script = f"{GUEST_SHARE}\\bv-mic-capture.ps1"
        code, body = guest.command(f"powershell -NoProfile -ExecutionPolicy Bypass -File {script} "
                                   f"-Seconds {CAPTURE_SECONDS}", timeout=300)
        summary["capture_exit"], summary["capture_output"] = code, body
        (summary["endpoints"], summary["capture"], summary["capture_error"], summary["exclusive_formats"],
         summary["shared"]) = parse_capture(body)
        log(f"capture exit={code}: " + " | ".join(line for line in body if line.startswith("MIC ")))
        with open(ctl, "a") as handle:
            handle.write("shutdown.exe /p /f\n")
        try:
            summary["runner_exit"] = proc.wait(timeout=240)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            summary["runner_exit"] = "killed after 240 s"
    except Exception as error:  # recorded; cleanup below
        summary["error"] = f"{type(error).__name__}: {error}"
        log(summary["error"])
        if proc is not None and proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=60)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
    finally:
        run_log = lane / "hvf" / "run.log"
        host_lines = []
        if created and run_log.exists():
            shutil.copyfile(run_log, args.out / "run.log")
            host_lines = run_log.read_text("utf-8", "replace").splitlines()
        if holder is not None:
            holder.kill()
            holder.wait()
        if created:
            shutil.rmtree(lane, ignore_errors=True)
        summary["lane_created"], summary["lane_deleted"] = created, created and not lane.exists()
        summary["checks"], summary["pass"] = verdict(summary.get("endpoints") or [], summary.get("capture"),
                                                     host_lines)
        summary["pass"] = summary["pass"] and "error" not in summary
        (args.out / "summary.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False))
        log(f"{'PASS' if summary['pass'] else 'FAIL'}: {json.dumps(summary['checks'])}; "
            f"shared: {json.dumps(summary.get('shared'))}; lane deleted: {summary['lane_deleted']}")
    return 0 if summary["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
