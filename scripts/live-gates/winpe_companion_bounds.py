"""Require complete bounded evidence before finishing a D4 collection."""
import json
import re
import stat
import subprocess

from bounded_output import BoundedOutput, read_status
from winpe_companion_process import run_owned

RAW_LIMIT = 64 * 1024 * 1024
PPM_LIMIT = 48 * 1024 * 1024 + 64
LOG_LIMIT = 512 * 1024 * 1024
WRAPPER_LIMIT = 16 * 1024 * 1024


def command(repo, records, clones, evidence):
    return ["bash", str(repo / "scripts/run-hvf-windows-installed-boot.sh"),
            "--target", str(clones["image"]), "--vars", str(clones["vars"]),
            "--placeholder-nsid1", str(clones["injector"]),
            "--firmware-code", str(records["firmware"][0]), "--evidence-dir", str(evidence),
            "--release", "--skip-build", "--watchdog-ms", "300000", "--max-reboots", "0",
            "--ram-mib", "4096", "--smp-cpus", "4", "--max-exits", "50000000",
            "--ramfb-samples", "1000,15000,30000,60000,90000,110000,120000",
            "--display-export-ppm", str(evidence / "latest.ppm"), "--no-guest-disk-harvest",
            "--diagnostic-output-bounds"]


def regular(path, limit):
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
        raise ValueError("unsafe or oversized bounded output")
    return info


def execute(command, private, environment, data):
    try:
        with BoundedOutput(private / "wrapper.log", private / "wrapper-bound.json", WRAPPER_LIMIT) as log:
            result = run_owned(command, env=environment, stdout=log,
                               stderr=subprocess.STDOUT, timeout=360)
            data.update(execution_exit_code=result.returncode, cleanup_complete=True)
        log.check()
    except ValueError:
        data["output_bounds_refused"] = True
        raise


def check_log(path, status_path, limit):
    metadata = read_status(status_path, limit)
    info = regular(path, limit)
    if info.st_size != metadata["stored_bytes"] or metadata["observed_bytes"] != info.st_size:
        raise ValueError("bounded log size disagrees with producer")


def verify(private):
    boot, frames = private / "boot", private / "boot/ramfb"
    marker = boot / "output-bound-refused.json"
    if marker.exists() or marker.is_symlink():
        raise ValueError("output ownership or logger refused")
    for name, status, limit in (("run.log", "run-log-bound.json", LOG_LIMIT),
                                ("target-stat.txt", "target-stat-bound.json", LOG_LIMIT),
                                ("cleanup.txt", "cleanup-bound.json", WRAPPER_LIMIT)):
        check_log(boot / name, boot / status, limit)
    check_log(private / "wrapper.log", private / "wrapper-bound.json", WRAPPER_LIMIT)
    for name in ("capture-bound-refused.json", "capture-bound-status.pending"):
        if (frames / name).exists() or (frames / name).is_symlink():
            raise ValueError("capture refusal or unfinished status")
    path = frames / "capture-bound-status.json"
    regular(path, 4096)
    def unique(pairs):
        value = {}
        for key, item in pairs:
            if key in value: raise ValueError("duplicate capture status key")
            value[key] = item
        return value
    policy = json.loads(path.read_bytes(), object_pairs_hook=unique)
    if (not isinstance(policy, dict) or set(policy) != {"schema", "raw_limit_bytes", "ppm_limit_bytes", "complete", "refused"}
            or policy["schema"] != "bridgevm.ramfb-capture-bound.v1"
            or type(policy["raw_limit_bytes"]) is not int or policy["raw_limit_bytes"] != RAW_LIMIT
            or type(policy["ppm_limit_bytes"]) is not int or policy["ppm_limit_bytes"] != PPM_LIMIT
            or policy["complete"] is not True or policy["refused"] is not False):
        raise ValueError("capture producer policy missing or refused")
    for log in (boot / "run.log", boot / "target-stat.txt", boot / "cleanup.txt", private / "wrapper.log"):
        tail = b""
        with log.open("rb") as stream:
            for chunk in iter(lambda: stream.read(65536), b""):
                scan = tail + chunk
                if any(word in scan for word in (b"DiagnosticOutputBoundRefusal", b"CapturePolicyError",
                                                b"CaptureByteLimit", b"CaptureAddressRange",
                                                b"ramfb checkpoint dump error:", b"ramfb framebuffer dump error:")):
                    raise ValueError("output failure cannot use a prior affirmative status")
                tail = scan[-128:]
    raw = {path.stem: path for path in frames.glob("*.xrgb8888")}
    ppm = {path.stem: path for path in frames.glob("*.ppm")}
    known = {"capture-bound-status.json"} | {p.name for p in raw.values()} | {p.name for p in ppm.values()}
    if any(path.name not in known for path in frames.iterdir()):
        raise ValueError("unknown capture artifact")
    if set(raw) != set(ppm) or len(raw) > 8:
        raise ValueError("partial or excessive fixed captures")
    for stem, path in ppm.items():
        info = regular(path, PPM_LIMIT)
        with path.open("rb") as stream:
            header = stream.read(64)
        match = re.match(rb"P6\n([0-9]{1,10}) ([0-9]{1,10})\n255\n", header)
        if not match:
            raise ValueError("invalid bounded PPM header")
        width, height = map(int, match.groups())
        if not 0 < width <= 0xffffffff or not 0 < height <= 0xffffffff:
            raise ValueError("invalid PPM dimensions")
        raw_bytes = regular(raw[stem], RAW_LIMIT).st_size
        if (info.st_size != len(match[0]) + width * height * 3
                or raw_bytes % height or raw_bytes // height < width * 4):
            raise ValueError("incomplete bounded framebuffer")
