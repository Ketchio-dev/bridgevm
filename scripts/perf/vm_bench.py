#!/usr/bin/env python3
"""Boot one Windows disk clone under BridgeVM or QEMU/HVF and measure it.

  vm_bench.py run --engine bridgevm|qemu --disk RAW --vars FD --out DIR
                  [--boots 2] [--cpus 4] [--ram-mib 6144] [--net] [--probe BIN]
  vm_bench.py compare A/results.json B/results.json

Every boot reuses one APFS clone of the disk and vars (canonical media are never
opened for writing); boot 1 is a warm-up that settles first-boot device setup.
Per boot: wall time from VMM launch to the resident agent's handshake, the
VMM's idle host CPU and RSS, then the shared in-guest suite. Results are live
single runs (evidence rank 2) on a shared host, never a criterion pass.
"""

from __future__ import annotations

import argparse
import base64
import calendar
import ctypes
import hashlib
import mmap
import struct
import math
import http.server
import json
import os
import platform
import re
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import textwrap
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from agent_wire import AgentWire  # noqa: E402
from guest_bench_commands import (  # noqa: E402
    HIGHER_IS_BETTER, OVERLAP_GUARD, SUITE, audio_loop, command_line, cursor_logger, desktop_mode, display_animation,
    net_download, parse_bvbench,
    stall_under_flush, stall_idle)

ROOT = Path(__file__).resolve().parents[2]
QEMU_FIRMWARE = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
QEMU_VARS_TEMPLATE = "/opt/homebrew/share/qemu/edk2-arm-vars.fd"
SHUTDOWN = "shutdown.exe /p /f"
# The probe prints this at PSCI SYSTEM_OFF, before it persists media and reports the stop.
GUEST_OFF = "guest powered off; host teardown follows"
SHUTDOWN_TIMEOUT_S = 240
# The app's command that switches viogpudo's hardware cursor on (from the next Windows start).
APP_HW_CURSOR = ROOT / "apps/macos/Sources/BridgeVMControl/HvfEngine/HvfDisplayDriverHardwareCursor.swift"


def clone(src: Path, dst: Path) -> None:
    libc = ctypes.CDLL("libc.dylib", use_errno=True)
    if libc.clonefile(os.fsencode(str(src)), os.fsencode(str(dst)), 0) != 0:
        raise OSError(ctypes.get_errno(), f"clonefile needs one APFS volume: {dst}")
    os.chmod(dst, 0o600)  # the staged source is read-only; its clone is the writable lane copy


def cpu_and_rss(pid: int) -> tuple[float, int] | None:
    fields = subprocess.run(["ps", "-o", "time=,rss=", "-p", str(pid)], capture_output=True, text=True).stdout.split()
    if len(fields) != 2:
        return None
    clock, days = fields[0], 0
    if "-" in clock:
        day, clock = clock.split("-")
        days = int(day)
    seconds = 0.0
    for part in clock.split(":"):
        seconds = seconds * 60 + float(part)
    return days * 86400 + seconds, int(fields[1])


def descendant(root_pid: int, needle: str) -> int | None:
    rows = subprocess.run(["ps", "-Ao", "pid=,ppid=,args="], capture_output=True, text=True).stdout.splitlines()
    parent, args = {}, {}
    for row in rows:
        pid, ppid, cmd = (row.split(None, 2) + [""])[:3]
        parent[int(pid)], args[int(pid)] = int(ppid), cmd
    for pid, cmd in args.items():
        hop = pid
        while needle in cmd.split(" ")[0] and hop in parent and hop > 1:
            hop = parent[hop]
            if hop == root_pid:
                return pid
    return None


class Engine:
    proc: subprocess.Popen
    vmm_pid: int | None

    def vmm_alive(self) -> bool:
        if self.vmm_pid is None or self.vmm_pid == self.proc.pid:
            return self.proc.poll() is None
        try:
            os.kill(self.vmm_pid, 0)
            return True
        except ProcessLookupError:
            return False

    def stop(self, deadline: float) -> bool:
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                return True
            time.sleep(0.5)
        os.killpg(self.proc.pid, signal.SIGKILL)
        self.proc.wait()
        return False


class Qemu(Engine):
    def __init__(self, args, disk: Path, vars_fd: Path):
        self.args, self.disk, self.vars_fd = args, disk, vars_fd

    def start(self, boot_dir: Path) -> None:
        sock = f"/tmp/bvbench-{os.getpid()}-{boot_dir.name}.sock"
        self.qmp_path = sock + ".qmp"
        cmd = [
            "qemu-system-aarch64", "-name", "bvbench", "-machine", "virt,highmem=on,gic-version=3",
            "-accel", "hvf", "-cpu", "host", "-smp", str(self.args.cpus), "-m", str(self.args.ram_mib),
            "-drive", f"if=pflash,format=raw,unit=0,readonly=on,file={QEMU_FIRMWARE}",
            "-drive", f"if=pflash,format=raw,unit=1,file={self.vars_fd}",
            "-drive", f"if=none,id=disk0,format=raw,file={self.disk},discard=unmap,detect-zeroes=unmap",
            "-device", "nvme,drive=disk0,serial=bvbench0,bootindex=0", "-device", "ramfb",
            "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0", "-device", "usb-tablet,bus=xhci.0",
            "-device", "virtio-serial-pci,id=vser",
            "-chardev", f"socket,id=agent,path={sock},server=on,wait=off",
            "-device", "virtserialport,bus=vser.0,chardev=agent,name=org.bridgevm.agent.0",
            "-display", "none", "-monitor", "none", "-serial", f"file:{boot_dir / 'serial.log'}",
            "-rtc", f"base={self.args.guest_date}", "-qmp", f"unix:{sock}.qmp,server=on,wait=off",
        ] + (["-nic", "user,model=virtio-net-pci"] if self.args.net else ["-nic", "none"]) + [
            arg.replace("{boot}", str(boot_dir)) for arg in self.args.qemu_arg]
        log = open(boot_dir / "qemu.log", "w")
        self.t0 = time.monotonic()
        self.proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        self.vmm_pid = self.proc.pid
        self.wire = AgentWire(sock, self.t0 + 60)

    def wait_ready(self, deadline: float) -> dict:
        self.wire.wait_ready(deadline)
        return {"ready_wall_ms": round((time.monotonic() - self.t0) * 1000)}

    def run(self, command: str, deadline: float) -> tuple[int, str]:
        return self.wire.run(command, deadline)

    def request_shutdown(self) -> None:
        self.wire.send("RUN " + base64.b64encode(SHUTDOWN.encode()).decode())

    def inject_moves(self, points) -> None:
        """usb-tablet absolute moves through QMP input-send-event at the given pace."""
        qmp = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        qmp.connect(self.qmp_path)
        reader = qmp.makefile("rb")
        reader.readline()
        qmp.sendall(b'{"execute":"qmp_capabilities"}\n')
        reader.readline()
        threading.Thread(target=lambda: [None for _ in iter(reader.readline, b"")], daemon=True).start()
        for x, y, due in points:
            time.sleep(max(0.0, due - time.monotonic()))
            event = {"execute": "input-send-event", "arguments": {"events": [
                {"type": "abs", "data": {"axis": "x", "value": x}},
                {"type": "abs", "data": {"axis": "y", "value": y}}]}}
            qmp.sendall(json.dumps(event).encode() + b"\n")
        qmp.close()


class BridgeVM(Engine):
    def __init__(self, args, disk: Path, vars_fd: Path):
        self.args, self.disk, self.vars_fd = args, disk, vars_fd

    def start(self, boot_dir: Path) -> None:
        self.ctl = boot_dir / "ctl.txt"
        self.ctl.write_text("")
        self.input_ctl = boot_dir / "input.ctl"
        self.input_ctl.write_text("")
        evidence = boot_dir / "hvf"
        self.log, self.offset, self.pending = evidence / "run.log", 0, ""
        cmd = [
            "bash", str(ROOT / "scripts/run-hvf-windows-installed-boot.sh"), "--target", str(self.disk),
            "--vars", str(self.vars_fd), "--evidence-dir", str(evidence), "--release", "--skip-build",
            "--smp-cpus", str(self.args.cpus), "--ram-mib", str(self.args.ram_mib),
            # BOOT_TIMER (for the READY milestone) would otherwise checksum the framebuffer every second
            # for the whole run; the product never enables it, so keep it out of the idle numbers.
            "--boot-timer-desktop-agent", "--boot-timer-ramfb-ms", str(self.args.boot_timer_ms),
            "--agent-service-control", str(self.ctl),
            "--watchdog-ms", "3600000", "--no-guest-disk-harvest",
        ] + (["--virtio-net"] if self.args.net else []) + (
            [] if self.args.no_xhci else ["--enable-xhci", "--input-control", str(self.input_ctl)])
        self.display_fb = evidence / "display.fb"
        self.display_cursor = evidence / "display.cursor"  # the device's hardware cursor, beside display.fb
        self.sent_moves = []
        if self.args.display:  # the app's export settings (HvfEngineConfig): frames on request, 33 ms fallback
            cmd += ["--display-export-ms", "100", "--ramfb-display-export-ms", "33",
                    "--display-export-fb", str(self.display_fb)]
        epoch = calendar.timegm(time.strptime(self.args.guest_date, "%Y-%m-%dT%H:%M:%S"))
        env = dict(os.environ, BRIDGEVM_PREBUILT_PROBE=str(self.args.probe), BRIDGEVM_GUEST_RTC_EPOCH=str(epoch))
        if self.args.audio:  # the HDA device without a host sink: silent on the host
            env["BRIDGEVM_HDA"] = "1"
        env.update(item.split("=", 1) for item in self.args.bvm_env)
        runner_log = open(boot_dir / "runner.log", "w")
        self.t0 = time.monotonic()
        self.proc = subprocess.Popen(cmd, cwd=ROOT, env=env, stdout=runner_log, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        self.vmm_pid = None

    def _lines(self, deadline: float, poll: float = 0.25):
        while time.monotonic() < deadline:
            if self.log.exists():
                with open(self.log, "rb") as handle:
                    handle.seek(self.offset)
                    data = handle.read()
                self.offset += len(data)
                self.pending += data.decode("utf-8", "replace")
                *done, self.pending = self.pending.split("\n")
                yield from done
            if self.proc.poll() is not None:
                raise RuntimeError(f"BridgeVM runner exited {self.proc.returncode}")
            time.sleep(poll)
        raise TimeoutError("BridgeVM deadline")

    def wait_ready(self, deadline: float) -> dict:
        result = {}
        for line in self._lines(deadline):
            if line.startswith("BVAGENT READY ") and "ready_wall_ms" not in result:
                result["ready_wall_ms"] = round((time.monotonic() - self.t0) * 1000)
            if line.startswith("BOOT_TIMER milestone name=desktop source=agent"):
                result["boot_timer_desktop_ms"] = int(line.split("elapsed_ms=")[1].split()[0])
            if line.startswith("BVAGENT SERVICE start"):
                self.vmm_pid = descendant(self.proc.pid, "hvf_gic_boot_probe")
                return result
        return result

    def run(self, command: str, deadline: float) -> tuple[int, str]:
        with open(self.ctl, "a") as handle:
            handle.write(command + "\n")
        # The runner logs input commands without their payload (VERB <uuid>).
        label = " ".join(command.split(" ")[:2]) if command.split(" ")[0].endswith("INPUT") else command
        head, body, code = f"BVAGENT CMD {label} exit=", None, 0
        # Reply latency is measured and QEMU's socket is immediate: poll the log
        # at 0.5 ms so the harness adds ~0.25 ms on average, not ~1 ms.
        for line in self._lines(deadline, poll=0.0005):
            if body is None and line.startswith(head):
                body, code = [], int(line[len(head):])
            elif body is not None and line == f"BVAGENT END {label}":
                return code, "\n".join(body)
            elif body is not None:
                body.append(line)
        raise TimeoutError(command[:60])

    def request_shutdown(self) -> None:
        with open(self.ctl, "a") as handle:
            handle.write(SHUTDOWN + "\n")

    def inject_moves(self, points) -> None:
        """xHCI tablet moves through the live input control file (the legacy route)."""
        with open(self.input_ctl, "a") as handle:
            for x, y, due in points:
                time.sleep(max(0.0, due - time.monotonic()))
                handle.write(f"POINTER move:{x}x{y}\n")
                handle.flush()
                self.sent_moves.append((x, y, time.clock_gettime_ns(time.CLOCK_UPTIME_RAW)))

    def guest_off_seen(self) -> bool:
        """PSCI SYSTEM_OFF reached the VMM; the rest of shutdown_ms is host teardown."""
        try:
            for line in self._lines(time.monotonic() + 0.3):
                if line == GUEST_OFF:
                    return True
        except (TimeoutError, RuntimeError):
            pass
        return False


class _Blob(http.server.BaseHTTPRequestHandler):
    payload = os.urandom(1 << 20) * 256

    def do_GET(self):  # noqa: N802
        self.send_response(200)
        self.send_header("Content-Length", str(len(self.payload)))
        self.end_headers()
        self.wfile.write(self.payload)

    def log_message(self, *_):
        pass


def blob_server() -> str:
    """Serve 256 MiB on the host's LAN address, which both NATs reach as an external host."""
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    probe.connect(("192.0.2.1", 9))
    address = probe.getsockname()[0]
    probe.close()
    server = http.server.ThreadingHTTPServer((address, 0), _Blob)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return f"http://{address}:{server.server_address[1]}/blob"


def circle(count: int, hz: float, start: float):
    return [(16384 + int(8000 * math.cos(i / 20)), 16384 + int(8000 * math.sin(i / 20)), start + i / hz)
            for i in range(count)]


def pointer_test(engine: Engine, record: dict) -> None:
    """Device route: 120 Hz absolute moves for 5 s while a guest logger counts cursor changes.
    Agent route: sequential trivial RUN round trips for 5 s (one command in flight, like the app's
    ordered input queue, which waits for each receipt)."""
    injector = threading.Thread(target=engine.inject_moves, args=(circle(600, 120.0, time.monotonic() + 1.5),))
    injector.start()
    seen = {}  # with --display: frames reaching the host while only the cursor moves (60 Hz consumer)
    watcher = cursor_watcher = None
    states = []  # with the guest's hardware cursor: what display.cursor published meanwhile
    if isinstance(engine, BridgeVM) and engine.display_fb.exists():
        watcher = threading.Timer(2.5, lambda: seen.update(consume_display(engine.display_fb, 3.0, hz=60.0)))
        watcher.start()
        if engine.display_cursor.exists():
            cursor_watcher = threading.Timer(2.5, lambda: states.extend(watch_cursor(engine.display_cursor, 3.0)))
            cursor_watcher.start()
    _, text = engine.run(command_line(cursor_logger(8000)), time.monotonic() + 120)
    injector.join()
    for timer in (watcher, cursor_watcher):
        if timer is not None:
            timer.join()  # a Timer's join waits for its function too
    record["bench"].update({k: v for k, (v, _) in parse_bvbench(text).items()})
    record["bench"].update({"pointer-" + k: v for k, v in seen.items()})
    if states:
        with open(engine.display_fb, "rb") as handle:  # the guest frame's size, for the moves' pixels
            width, height = struct.unpack_from("<2I", handle.read(16), 8)
        record["bench"].update(cursor_metrics(states, engine.sent_moves, width, height))
    rtts, stop = [], time.monotonic() + 5
    echo = "echo bv"  # both engines wrap a plain command line into RUN
    while time.monotonic() < stop:
        started = time.monotonic()
        code, text = engine.run(echo, time.monotonic() + 30)
        if code != 0 or "bv" not in text:
            record["errors"].append(f"agent rtt: exit={code} {text.strip()[:120]}")
            break
        rtts.append((time.monotonic() - started) * 1000)
    if rtts:
        rtts.sort()
        record["bench"].update({"agent-cmd-hz": round(len(rtts) / 5, 1),
                                "agent-rtt-p50": round(rtts[len(rtts) // 2], 1),
                                "agent-rtt-p95": round(rtts[int(len(rtts) * 0.95)], 1)})


def watch_cursor(path: Path, seconds: float) -> list[tuple[int, int, int]]:
    """Read display.cursor at about 1 kHz like the app's display tick (even sequence at byte 24): each
    new visible position of the guest's pointer (the sprite's top left plus the hot spot) with its
    publish time (CLOCK_UPTIME_RAW ns at byte 48)."""
    fd = os.open(path, os.O_RDONLY)
    try:
        view = mmap.mmap(fd, 64, mmap.MAP_SHARED, mmap.PROT_READ)
        states, last_seq, last_pos = [], None, None
        stop = time.perf_counter() + seconds
        while time.perf_counter() < stop and struct.unpack_from("<I", view, 0)[0] == 0x42564352:
            seq = struct.unpack_from("<Q", view, 24)[0]
            if seq % 2 == 0 and seq != last_seq:
                _, flags, x, y, published, hot_x, hot_y = struct.unpack_from("<IIiiQII", view, 32)
                if struct.unpack_from("<Q", view, 24)[0] == seq:  # not torn by a publish meanwhile
                    last_seq = seq
                    if flags & 1 and (x + hot_x, y + hot_y) != last_pos:
                        last_pos = (x + hot_x, y + hot_y)
                        states.append((published, *last_pos))
            time.sleep(0.001)
        view.close()
    finally:
        os.close(fd)
    return states


def cursor_metrics(states, sent, width: int, height: int) -> dict:
    """Hardware cursor moves reaching the host while the pointer moves, and each one's lag: from writing
    the move to input.ctl to the device publishing it. A position belongs to the latest sent move within
    4 guest pixels of it in the last 0.5 s (one circle step is about 15 pixels at 1280x800; a lap
    takes about 1 s)."""
    out = {}
    if len(states) >= 3:
        span = (states[-1][0] - states[0][0]) / 1e9
        gaps = sorted((b[0] - a[0]) / 1e6 for a, b in zip(states, states[1:]))
        out.update({"pointer-cursor-hz": round((len(states) - 1) / span, 1) if span > 0 else 0.0,
                    "pointer-cursor-gap-ms-p50": round(gaps[len(gaps) // 2], 1),
                    "pointer-cursor-gap-ms-p95": round(gaps[int(len(gaps) * 0.95)], 1)})
    lags, sent = [], list(sent)
    for published, x, y in states:
        for sx, sy, ns in reversed(sent):
            if ns > published or published - ns > 500e6:
                continue
            if math.hypot(sx * width / 32767 - x, sy * height / 32767 - y) <= 4:
                lags.append((published - ns) / 1e6)
                break
    if lags:
        lags.sort()
        out.update({"pointer-cursor-lag-ms-p50": round(lags[len(lags) // 2], 1),
                    "pointer-cursor-lag-ms-p95": round(lags[int(len(lags) * 0.95)], 1),
                    "pointer-cursor-matched-pct": round(100 * len(lags) / len(states), 1)})
    return out


def hw_cursor_command() -> str:
    """The app's own command, from its one raw multi-line string, so the bench switches the cursor on
    exactly as the app does (the app puts the value to write, `$want`, in front of it)."""
    script = APP_HW_CURSOR.read_text().split('#"""\n', 1)[1].split('"""#', 1)[0]
    return command_line("$want = 1\n" + textwrap.dedent(script))


def save_display_frame(path: Path, out: Path) -> None:
    """The last published BVFB frame (XRGB8888) as a PPM."""
    data = path.read_bytes()
    magic, _, width, height, stride = struct.unpack_from("<5I", data, 0)
    if magic != 0x42564642 or not width or not height:
        return
    rgb = bytearray(width * height * 3)
    for y in range(height):
        row = data[64 + y * stride:64 + y * stride + width * 4]
        rgb[y * width * 3:(y + 1) * width * 3] = bytes(
            b for x in range(width) for b in (row[4 * x + 2], row[4 * x + 1], row[4 * x]))
    out.write_bytes(b"P6 %d %d 255\n" % (width, height) + bytes(rgb))


def consume_display(path: Path, seconds: float, hz: float = 120.0) -> dict:
    """Read display.fb like the app's display link: each tick take the published frame (even
    sequence at byte 24, publish time at 48) and post a frame request (CLOCK_UPTIME_RAW ns at 32)."""
    fd = os.open(path, os.O_RDWR)
    try:
        view = mmap.mmap(fd, 64, mmap.MAP_SHARED, mmap.PROT_READ)
        if struct.unpack_from("<I", view, 0)[0] != 0x42564642:
            return {}
        frames, last_seq, ages, stamps = 0, None, [], []
        start = time.perf_counter()
        tick = start
        while time.perf_counter() - start < seconds:
            seq, published = struct.unpack_from("<Q", view, 24)[0], struct.unpack_from("<Q", view, 48)[0]
            now = time.clock_gettime_ns(time.CLOCK_UPTIME_RAW)
            if seq % 2 == 0 and seq != last_seq:
                if last_seq is not None:
                    frames += 1
                    stamps.append(published)
                    ages.append((now - published) / 1e6)
                last_seq = seq
            os.pwrite(fd, struct.pack("<Q", now), 32)
            tick += 1 / hz
            time.sleep(max(0.0, tick - time.perf_counter()))
        view.close()
    finally:
        os.close(fd)
    gaps = sorted((b - a) / 1e6 for a, b in zip(stamps, stamps[1:]))
    ages.sort()
    out = {"display-fps": round(frames / seconds, 1)}
    if gaps:
        out.update({"display-frame-ms-p50": round(gaps[len(gaps) // 2], 1),
                    "display-frame-ms-p95": round(gaps[int(len(gaps) * 0.95)], 1),
                    "display-age-ms-p95": round(ages[int(len(ages) * 0.95)], 1)})
    return out


def display_test(engine: Engine, record: dict) -> None:
    host = {}
    consumer = None
    if isinstance(engine, BridgeVM) and engine.display_fb.exists():
        consumer = threading.Thread(target=lambda: host.update(consume_display(engine.display_fb, 6.0)))
        threading.Timer(3.0, consumer.start).start()  # the window is up and animating by then
    _, text = engine.run(command_line(display_animation(10000)), time.monotonic() + 120)
    if consumer is not None:
        consumer.join()
    record["bench"].update({k: v for k, (v, _) in parse_bvbench(text).items()})
    record["bench"].update(host)


def guard_pids(engine: Engine):
    """The agent's PowerShell children other than the guard itself, or None when the guest gives no answer."""
    _, text = engine.run(command_line(OVERLAP_GUARD), time.monotonic() + 60)
    match = re.search(r"BVGUARD pids=([0-9,]*)", text)
    return None if match is None else {int(p) for p in match.group(1).split(",") if p}


def overlap_check(engine: Engine, record: dict, baseline, window: str) -> None:
    """--overlap-guard: a guest process started after READY and still alive as `window` opens marks the boot invalid."""
    pids = guard_pids(engine)
    if pids is None:
        record["errors"].append(f"overlap guard before {window}: no answer")
    elif pids - baseline:
        record["errors"].append(f"overlap guard before {window}: processes {sorted(pids - baseline)} still running")
        record.setdefault("invalid", f"a bench process was still running when {window} opened")


def one_boot(engine: Engine, boot_dir: Path, args) -> dict:
    record = {"boot": int(boot_dir.name.split("-")[1]), "loadavg_start": os.getloadavg(), "bench": {}, "errors": []}
    try:
        engine.start(boot_dir)
        record.update(engine.wait_ready(time.monotonic() + args.ready_timeout))
        baseline = (guard_pids(engine) or set()) if args.overlap_guard else None
        time.sleep(args.settle_s)
        if baseline is not None:
            overlap_check(engine, record, baseline, "idle")
        first = cpu_and_rss(engine.vmm_pid) if engine.vmm_pid else None
        diag = os.environ.get("BVBENCH_SAMPLE") and engine.vmm_pid
        if diag:  # where the idle time goes: per-thread times around the window, plus a stack sample
            threads_before = subprocess.run(["ps", "-M", "-p", str(engine.vmm_pid)], capture_output=True,
                                            text=True).stdout
            subprocess.Popen(["/usr/bin/sample", str(engine.vmm_pid), "5", "-file", str(boot_dir / "idle-sample.txt")],
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(args.idle_s)
        second = cpu_and_rss(engine.vmm_pid) if engine.vmm_pid else None
        if diag:
            after = subprocess.run(["ps", "-M", "-p", str(engine.vmm_pid)], capture_output=True, text=True).stdout
            (boot_dir / "idle-threads.txt").write_text(threads_before + "\n----\n" + after)
        if first and second:
            record["idle_cpu_pct"] = round((second[0] - first[0]) / args.idle_s * 100, 1)
            record["rss_mib"] = round(second[1] / 1024)
        if args.pointer:
            try:
                pointer_test(engine, record)
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"pointer: {type(error).__name__}: {error}")
        if args.desktop_mode:
            try:
                _, text = engine.run(command_line(desktop_mode(args.desktop_mode == "native")), time.monotonic() + 60)
                record["bench"].update({k: v for k, (v, _) in parse_bvbench(text).items()})
                if isinstance(engine, BridgeVM) and engine.display_fb.exists():
                    save_display_frame(engine.display_fb, boot_dir / "desktop.pnm")
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"desktop-mode: {type(error).__name__}: {error}")
        if args.display:
            try:
                display_test(engine, record)
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"display: {type(error).__name__}: {error}")
        if args.shutdown_diag:  # how the previous shutdown of this disk spent its time (guest's own log)
            script = ("Get-WinEvent -LogName 'Microsoft-Windows-Diagnostics-Performance/Operational' -MaxEvents 12 "
                      "-ErrorAction SilentlyContinue | Where-Object { $_.Id -ge 200 -and $_.Id -le 206 } | "
                      "ForEach-Object { $x = [xml]$_.ToXml(); $d = ($x.Event.EventData.Data | Where-Object { $_.'#text' } | "
                      "ForEach-Object { $_.Name + '=' + $_.'#text' }) -join ' '; "
                      "'{0:o} {1} {2}' -f $_.TimeCreated, $_.Id, $d }")
            try:
                _, text = engine.run(command_line(script), time.monotonic() + 60)
                record["shutdown_diag"] = text.strip()[-4000:]
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"shutdown-diag: {type(error).__name__}: {error}")
        if args.audio:
            try:
                code, text = engine.run(command_line(audio_loop(1800)), time.monotonic() + 60)
                if code != 0 or "BVAUDIO rv=0" not in text:
                    record["errors"].append(f"audio: exit={code} {text.strip()[:200]}")
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"audio: {type(error).__name__}: {error}")
        if args.stall_idle:
            try:
                _, text = engine.run(command_line(stall_idle(8000)), time.monotonic() + 120)
                record["bench"].update({k: v for k, (v, _) in parse_bvbench(text).items()})
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"stall-idle: {type(error).__name__}: {error}")
        if args.stall:
            try:
                _, text = engine.run(command_line(stall_under_flush(8000)), time.monotonic() + 120)
                record["bench"].update({k: v for k, (v, _) in parse_bvbench(text).items()})
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"stall: {type(error).__name__}: {error}")
        suite = SUITE + ([("net-download", net_download(args.blob_url, hashlib.sha256(_Blob.payload).hexdigest()))]
                         if args.net else [])
        only = None if args.only is None else set(args.only.split(","))
        for name, script in ([] if args.skip_suite else [t for t in suite if only is None or t[0] in only]):
            if baseline is not None:
                overlap_check(engine, record, baseline, name)
            started = time.monotonic()
            try:
                code, text = engine.run(command_line(script), time.monotonic() + 600)
                values = parse_bvbench(text)
                record["bench"].update({metric: value for metric, (value, _) in values.items()})
                if not values:
                    record["errors"].append(f"{name}: exit={code} no value: {text.strip()[:300]}")
            except (TimeoutError, RuntimeError, ConnectionError) as error:
                record["errors"].append(f"{name}: {error}")
                break
            record.setdefault("bench_wall_s", {})[name] = round(time.monotonic() - started, 2)
        if args.hw_cursor and record["boot"] == 1:  # applies from the next Windows start: boot 2 on
            try:
                _, text = engine.run(hw_cursor_command(), time.monotonic() + 120)
                record["hw_cursor"] = next((line.strip() for line in text.splitlines()
                                            if line.startswith("BVCURSOR ")), "")
                if record["hw_cursor"] not in ("BVCURSOR result=set", "BVCURSOR result=already"):
                    record["errors"].append(f"hw-cursor: {text.strip()[:300]}")
            except (TimeoutError, RuntimeError, ConnectionError, OSError) as error:
                record["errors"].append(f"hw-cursor: {type(error).__name__}: {error}")
        stop_started = time.monotonic()
        engine.request_shutdown()
        while engine.vmm_alive() and time.monotonic() < stop_started + SHUTDOWN_TIMEOUT_S:
            if "guest_off_ms" not in record and getattr(engine, "guest_off_seen", lambda: False)():
                record["guest_off_ms"] = round((time.monotonic() - stop_started) * 1000)
            time.sleep(0.1)
        if engine.vmm_alive():  # no shutdown time: the shutdown never completed
            record["errors"].append(f"shutdown: the VMM still ran {SHUTDOWN_TIMEOUT_S} s after the request")
        else:
            record["shutdown_ms"] = round((time.monotonic() - stop_started) * 1000)
        record["clean_exit"] = engine.stop(time.monotonic() + SHUTDOWN_TIMEOUT_S)
        if isinstance(engine, BridgeVM) and engine.log.exists():  # the probe's end-of-run cursor counts
            counts = re.findall(rb"virtio-gpu cursor updates=(\d+) moves=(\d+) hides=(\d+)", engine.log.read_bytes())
            if counts:
                record["virtio_gpu_cursor"] = dict(zip(("updates", "moves", "hides"), map(int, counts[-1])))
    except BaseException as error:
        record["errors"].append(f"boot: {type(error).__name__}: {error}")
        if getattr(engine, "proc", None) is not None and engine.proc.poll() is None:
            os.killpg(engine.proc.pid, signal.SIGKILL)
            engine.proc.wait()
        if not isinstance(error, Exception):  # SIGTERM (SystemExit) or Ctrl-C ends the campaign
            raise
    record["loadavg_end"] = os.getloadavg()
    return record


def other_vmms(text) -> str:
    """VMM processes already running, one line each with its command line.

    Matches exact process names: matching command lines (pgrep -f) also caught
    waiter shells whose arguments name a VMM, and those filled the record."""
    pids = text(["pgrep", "-x", "hvf_gic_boot_probe|qemu-system-aarch64|tart|prl_vm_app"]).split()
    if not pids:
        return ""
    rows = text(["ps", "-o", "pid=,command=", "-p", ",".join(pids)]).splitlines()
    return "\n".join(row.strip()[:160] for row in rows)[:1200]


def identity(args) -> dict:
    def text(cmd):
        return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()

    ident = {
        "engine": args.engine, "cpus": args.cpus, "ram_mib": args.ram_mib, "net": args.net,
        "guest_date": args.guest_date, "bvm_env": args.bvm_env,
        "host": text(["/usr/sbin/sysctl", "-n", "machdep.cpu.brand_string"]), "macos": platform.mac_ver()[0],
        "source_commit": text(["git", "-C", str(ROOT), "rev-parse", "HEAD"]), "disk": Path(args.disk).name,
        "other_vmms": other_vmms(text),
    }
    if args.engine == "bridgevm":
        ident["probe_sha256"] = hashlib.sha256(Path(args.probe).read_bytes()).hexdigest()
    else:
        ident["qemu"] = text(["qemu-system-aarch64", "--version"]).splitlines()[0]
    return ident


def run(args) -> None:
    out = Path(args.out).resolve()  # the BridgeVM runner starts in ROOT
    results = {"identity": identity(args), "boots": []}
    out.mkdir(parents=True)
    disk, vars_fd = out / "disk.raw", out / "vars.fd"
    if args.engine == "bridgevm":  # a private signed copy: later rebuilds cannot change a running campaign
        (out / "bin").mkdir()
        shutil.copy2(args.probe, out / "bin" / "hvf_gic_boot_probe")
        args.probe = str(out / "bin" / "hvf_gic_boot_probe")
        subprocess.run(["codesign", "--sign", "-", "--force", "--entitlements",
                        str(ROOT / "apps/macos/HvfRunner.entitlements"), args.probe], check=True, capture_output=True)
        results["identity"]["probe_sha256"] = hashlib.sha256(Path(args.probe).read_bytes()).hexdigest()
    engine = (Qemu if args.engine == "qemu" else BridgeVM)(args, disk, vars_fd)
    args.blob_url = blob_server() if args.net else None
    try:
        clone(Path(args.disk), disk)
        if args.engine == "qemu":  # QEMU's own empty variable store, as a fresh QEMU VM gets
            shutil.copyfile(QEMU_VARS_TEMPLATE, vars_fd)
        else:
            clone(Path(args.vars), vars_fd)
        for boot in range(1, args.boots + 1):
            boot_dir = out / f"boot-{boot}"
            boot_dir.mkdir()
            results["boots"].append(one_boot(engine, boot_dir, args))
            (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            print(json.dumps(results["boots"][-1]), flush=True)
    finally:
        if not args.keep_media:
            disk.unlink(missing_ok=True)
            vars_fd.unlink(missing_ok=True)


def summary(path: str) -> dict:
    data = json.loads(Path(path).read_text())
    boots = [b for b in data["boots"] if b["boot"] > 1 or len(data["boots"]) == 1]
    for boot in boots:  # results written before multi-metric scripts were recorded
        for error in boot["errors"]:
            boot["bench"].update({k: v for k, (v, _) in parse_bvbench(error.split("no value: ", 1)[-1]).items()})
    keys = ["ready_wall_ms", "boot_timer_desktop_ms", "idle_cpu_pct", "rss_mib", "guest_off_ms", "shutdown_ms"]
    table = {k: [b[k] for b in boots if k in b] for k in keys}
    for name in sorted({n for b in boots for n in b["bench"]}):
        table[name] = [b["bench"][name] for b in boots if name in b["bench"]]
    return {"identity": data["identity"], "median": {k: statistics.median(v) for k, v in table.items() if v},
            "samples": {k: v for k, v in table.items() if v}}


def compare(a: str, b: str) -> None:
    left, right = summary(a), summary(b)
    names = (left["identity"]["engine"], right["identity"]["engine"])
    print(f"{'metric':28} {names[0]:>12} {names[1]:>12}  better")
    lower_better = {"ready_wall_ms", "boot_timer_desktop_ms", "idle_cpu_pct", "rss_mib", "guest_off_ms", "shutdown_ms",
                    "cursor-interval-p50", "cursor-interval-p95", "agent-rtt-p50", "agent-rtt-p95",
                    "stall-max", "stall-p99", "stall-over-20ms", "stall-idle-max", "stall-idle-p99",
                    "stall-idle-over-5ms", "display-frame-ms-p50", "display-frame-ms-p95",
                    "display-age-ms-p95", "pointer-display-frame-ms-p50", "pointer-display-frame-ms-p95",
                    "pointer-display-age-ms-p95"}
    for key in left["median"].keys() | right["median"].keys():
        x, y = left["median"].get(key), right["median"].get(key)
        if x is None or y is None:
            print(f"{key:28} {x!s:>12} {y!s:>12}")
            continue
        higher = key in HIGHER_IS_BETTER and key not in lower_better
        win = names[0] if (x > y) == higher and x != y else names[1] if x != y else "tie"
        ratio = (x / y if higher else y / x) if x and y else float("nan")
        print(f"{key:28} {x:>12.1f} {y:>12.1f}  {win} ({ratio:.2f}x {names[0]}/{names[1]})")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="mode", required=True)
    r = sub.add_parser("run")
    r.add_argument("--engine", choices=["bridgevm", "qemu"], required=True)
    r.add_argument("--disk", required=True)
    r.add_argument("--vars", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--probe", default=str(ROOT / "target/release/examples/hvf_gic_boot_probe"))
    r.add_argument("--boots", type=int, default=2)
    r.add_argument("--cpus", type=int, default=4)
    r.add_argument("--ram-mib", type=int, default=6144)
    r.add_argument("--net", action="store_true")
    r.add_argument("--pointer", action="store_true", help="cursor update rate per input route")
    r.add_argument("--hw-cursor", action="store_true",
                   help="BridgeVM virtio-gpu: switch viogpudo's hardware cursor on at the end of boot 1 (as the app does)")
    r.add_argument("--desktop-mode", choices=["report", "native"],
                   help="Windows desktop resolution (native: first switch to the display's largest mode)")
    r.add_argument("--skip-suite", action="store_true", help="only boot, idle, optional pointer/net, shutdown")
    r.add_argument("--overlap-guard", action="store_true",
                   help="mark a boot invalid if a guest process started after READY is still running when the idle"
                        " window or a suite test opens")
    r.add_argument("--only", help="comma-separated suite tests to run (e.g. mem-first-touch,net-download)")
    r.add_argument("--boot-timer-ms", type=int, default=60000, help="BridgeVM framebuffer sample period (diagnostics)")
    r.add_argument("--no-xhci", action="store_true", help="BridgeVM without the USB controller (diagnostics)")
    r.add_argument("--qemu-arg", action="append", default=[], help="extra QEMU argument; {boot} expands to the boot dir")
    r.add_argument("--display", action="store_true", help="guest animation; BridgeVM also counts frames reaching the host")
    r.add_argument("--stall", action="store_true", help="guest timer gaps while another thread flushes files")
    r.add_argument("--stall-idle", action="store_true", help="the same timer gaps with no other load")
    r.add_argument("--shutdown-diag", action="store_true", help="record the guest's own shutdown timing events")
    r.add_argument("--audio", action="store_true",
                   help="loop silent guest audio during the tests (BridgeVM: HDA on, which prints the platform lock holds)")
    r.add_argument("--settle-s", type=float, default=60)
    r.add_argument("--idle-s", type=float, default=30)
    r.add_argument("--ready-timeout", type=float, default=600)
    r.add_argument("--keep-media", action="store_true")
    r.add_argument("--bvm-env", action="append", default=[], metavar="KEY=VALUE",
                   help="extra BridgeVM probe environment forwarded by the runner (experiments)")
    # The canonical images' account passwords expire after their preparation date.
    r.add_argument("--guest-date", default="2026-08-01T00:00:00", help="guest RTC start, UTC")
    c = sub.add_parser("compare")
    c.add_argument("a")
    c.add_argument("b")
    args = parser.parse_args()
    unknown = set((getattr(args, "only", None) or "").split(",")) - {n for n, _ in SUITE} - {"net-download", ""}
    if unknown:  # a typo would boot every VM of a campaign and measure nothing
        parser.error(f"--only: unknown tests {sorted(unknown)}; known: {[n for n, _ in SUITE] + ['net-download']}")
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))  # kill the VM and drop media clones
    run(args) if args.mode == "run" else compare(args.a, args.b)


if __name__ == "__main__":
    main()
