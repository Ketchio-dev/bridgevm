#!/usr/bin/env python3
"""Every remaining SYSTEM_OFF and CoreAudio reader binds to the final host report.

Guest bytes reach run.log as agent output before the final report and as its
counted serial tail, and the probe exits 0 after a watchdog stop. Each forged
log meets every other requirement of its reader, so only the author of the stop
record or of the counters differs. An honest log in the runtime's current
format (hvf-terminal-stop-contract.py ties it to final_report.rs) still passes.
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
CLI = LIVE_GATES / "hvf_terminal_evidence.py"
SHELL = ROOT / "scripts/hvf-terminal-report.sh"
sys.path.insert(0, str(LIVE_GATES))
import windows_product_e2e_guest_evidence as guest_evidence  # noqa: E402
from hvf_guest_shutdown import guest_shutdown_observed  # noqa: E402
from hvf_stop_line import SYSTEM_OFF  # noqa: E402
from hvf_terminal_report import LOG_LIMIT, system_off_offset  # noqa: E402

BANNER = "=== EDK2 boot probe (with Apple hv_gic) ==="
WATCHDOG = "stop: watchdog (CANCELED)"
AGENT = ("BVAGENT READY host=BRIDGEVM t=1\nBVAGENT CMD whoami exit=0\nbridgevm\\user\nBVAGENT END whoami\n"
         "BVAGENT SERVICE start t=2\n")
WRITEBACK = "NVMe disk written back: /tmp/windows.raw (4096 bytes)\n"
STATS = ("hda CoreAudio stats: frames_rendered=96000 drops=0 dropped_bytes=0 format_drops=0 ring_full_drops=0 "
         "queue_stop_errors=0 queue_dispose_errors=0 callback_errors=0 callback_active_errors=0 "
         "callback_stopping_errors=0 callback_expected_stopping_errors=0 callback_unexpected_errors=0 "
         "callback_stopping_invalid_run_state=0 callback_stopping_queue_invalidated=0 "
         "callback_stopping_enqueue_during_reset=0 callback_stopping_disposal_pending=0 "
         "callback_stopping_unclassified=0\n")
TEARDOWN = ("hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
            "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n" + STATS)


def report(stop: str, serial: str = "UEFI firmware\r\n", tail: str = TEARDOWN) -> str:
    size = len(serial.encode())
    return (f"{WRITEBACK}REGS: pc=0x0 lr=0x0\n{BANNER}\n{stop}\n"
            f"exits: 1 (vtimer 0, psci 1, surplus-canceled 0), last PC: 0x0\nsymbol lines: 0\n"
            f"serial raw bytes: {size} output bytes: {size}\n--- serial (tail) ---\n{serial}\n--- end ---\n{tail}")


def agent_output(output: str) -> str:
    return AGENT.replace("bridgevm\\user", output)


GENUINE = AGENT + report(SYSTEM_OFF)
FORGED_STOP = {
    "serial tail after a watchdog stop": AGENT + report(WATCHDOG, f"Boot Manager\r\n{SYSTEM_OFF}\r\n"),
    "agent command output": agent_output(SYSTEM_OFF) + report(WATCHDOG),
    "agent output after a carriage return": agent_output(f"bridgevm\\user\r{SYSTEM_OFF}") + report(WATCHDOG),
    "record without a host report": agent_output(SYSTEM_OFF) + WRITEBACK + TEARDOWN,
}
# The host printed no CoreAudio records; the guest printed passing ones.
FORGED_AUDIO = {
    "counters in the serial tail": AGENT + report(SYSTEM_OFF, f"boot\r\n{TEARDOWN}", ""),
    "counters in agent output": agent_output(TEARDOWN.rstrip("\n")) + report(SYSTEM_OFF, tail=""),
}


def honest(serial: bytes = b"UEFI firmware\r\n", tail: bytes = TEARDOWN.encode(), count: bytes | None = None,
           stop: bytes = SYSTEM_OFF.encode()) -> bytes:
    count = count if count is not None else b"serial raw bytes: %d output bytes: %d" % (len(serial), len(serial))
    return (AGENT.encode() + WRITEBACK.encode() + b"REGS: pc=0x0\n" + BANNER.encode() + b"\n" + stop + b"\n"
            + count + b"\n--- serial (tail) ---\n" + serial + b"\n--- end ---\n" + tail)


SHORT = b"hda CoreAudio stats: frames_rendered=1 drops=0 callback_errors=0\n"
# Framing corners the shell binding must decide exactly as system_off_offset does;
# the default serial tail is 15 bytes.
EDGES = {
    "legacy count": honest(count=b"serial bytes: 15"),
    "zero-padded count": honest(count=b"serial raw bytes: 00000015 output bytes: 00000015"),
    "raw count above output": honest(count=b"serial raw bytes: 16 output bytes: 15"),
    "count one byte short": honest(count=b"serial raw bytes: 14 output bytes: 14"),
    "NUL and invalid UTF-8 in the tail": honest(b"boot\x00\xff\xfe\r\n"),
    "empty serial and host tail": honest(b"", b""),
    "stop record with a carriage return": honest(stop=SYSTEM_OFF.encode() + b"\r"),
    "unterminated host record": honest()[:-1],
    "sixteen host records": honest(tail=SHORT * 16),
    "seventeen host records": honest(tail=SHORT * 17),
    "host tail over 4 KiB": honest(tail=STATS.encode() * 7 + b"hda CoreAudio stats: a=" + b"1" * 2000 + b"\n"),
    "blank host record": honest(tail=b"\n" + STATS.encode()),
    "footer without its newline": honest(tail=b"")[:-1],
    "banner at the start of the log": honest()[len(AGENT) + len(WRITEBACK) + len("REGS: pc=0x0\n"):],
}


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class StopReadersContract(unittest.TestCase):
    def accepted(self, reader, logs: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as temp:
            results = []
            for name, text in logs.items():
                directory = Path(temp) / str(len(results))
                directory.mkdir()
                (directory / "run.log").write_bytes(text.encode())
                results.append((name, reader(directory)))
            return [name for name, passed in results if passed]

    def assert_binds(self, reader, audio: bool = False) -> None:
        self.assertEqual(self.accepted(reader, {"genuine": GENUINE}), ["genuine"])
        self.assertEqual(self.accepted(reader, {**FORGED_STOP, **(FORGED_AUDIO if audio else {})}), [])
        with tempfile.TemporaryDirectory() as temp:
            (Path(temp) / "genuine.log").write_bytes(GENUINE.encode())
            (Path(temp) / "run.log").symlink_to(Path(temp) / "genuine.log")
            self.assertFalse(reader(Path(temp)), "run.log linked to a genuine log")

    def test_shell_and_python_bindings_decide_as_the_grammar(self):
        """The packaged runner has no Python, so its shell binding is checked against system_off_offset."""
        stop_contract = load("hvf_terminal_stop_contract", ROOT / "tests/integration/hvf-terminal-stop-contract.py")
        cases = {**{name: text.encode() for name, text in {**stop_contract.GENUINE, **stop_contract.FORGED,
                                                            "genuine": GENUINE, **FORGED_STOP, **FORGED_AUDIO}.items()},
                 **EDGES}
        self.assertEqual({name for name, data in cases.items() if system_off_offset(data) is not None},
                         {*stop_contract.GENUINE, "genuine", *FORGED_AUDIO, "zero-padded count",
                          "NUL and invalid UTF-8 in the tail", "empty serial and host tail", "sixteen host records"})
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "run.log"
            for name, data in cases.items():
                log.write_bytes(data)
                expected = 0 if system_off_offset(data) is not None else 1
                self.assertEqual((self.shell(log), self.python(log)), (expected, expected), name)
            serial = b"A" * (LOG_LIMIT - 1 - len(honest(b"", count=b"serial raw bytes: 99999999 output bytes: 99999999")))
            for size, expected in ((LOG_LIMIT - 1, 0), (LOG_LIMIT, 1)):
                data = honest(serial + b"A" * (size - LOG_LIMIT + 1))
                self.assertEqual(len(data), size)
                log.write_bytes(data)
                self.assertEqual((self.shell(log), self.python(log)), (expected, expected), size)

    def shell(self, log: Path) -> int:
        return subprocess.run(["/bin/bash", str(SHELL), "--require-system-off", str(log)], capture_output=True,
                              timeout=120).returncode

    def python(self, log: Path) -> int:
        return subprocess.run([sys.executable, str(CLI), "--require-system-off", str(log)], capture_output=True,
                              timeout=120).returncode

    def test_packaged_runner_uses_the_shell_binding(self):
        runner = (ROOT / "scripts/run-hvf-windows-installed-boot-runner.sh").read_text(encoding="utf-8")
        self.assertEqual(runner.count('/bin/bash "$ROOT/scripts/hvf-terminal-report.sh" --require-system-off '
                                      '"$EVIDENCE_DIR/run.log"'), 3)
        self.assertNotIn("(system off)", runner)
        for gate in ("write_host_pause_resume_gate", "write_agent_shutdown_gate", "write_agent_service_gate"):
            self.assertNotIn("python3", re.search(rf"^{gate}\(\) {{\n.*?^}}\n", runner, re.M | re.S)[0], gate)
        for package, line, count in (
                ("apps/macos/scripts/package-hvf-control-app.sh", "  run-hvf-windows-installed-boot-runner.sh "
                 "run-hvf-windows-installed-boot-package-policy.sh hvf-terminal-report.sh \\\n", 1),
                ("packaging/macos/build-debug-app-bundle.sh", "run-hvf-windows-installed-boot-runner.sh "
                 "hvf-terminal-report.sh; do\n", 2)):
            self.assertEqual((ROOT / package).read_text(encoding="utf-8").count(line), count, package)
        self.assertEqual(SHELL.read_text(encoding="utf-8").splitlines()[0], "#!/bin/bash")
        self.assertTrue(os.access(SHELL, os.X_OK))

    def test_cli_requires_the_final_report_stop(self):
        def reader(directory: Path) -> bool:
            result = subprocess.run([sys.executable, str(CLI), "--require-system-off", str(directory / "run.log")],
                                    capture_output=True, timeout=30)
            self.assertIn(result.returncode, (0, 1), result.stderr)
            return result.returncode == 0
        self.assert_binds(reader)
        self.assertFalse(reader(ROOT / "missing-run-log-directory"))
        self.assertNotEqual(subprocess.run([sys.executable, str(CLI)], capture_output=True, timeout=30).returncode, 0)

    def runner_gate(self, setup: str, output: str) -> None:
        def reader(directory: Path) -> bool:
            script = ('set -euo pipefail; source scripts/run-hvf-windows-installed-boot-runner.sh; EVIDENCE_DIR="$1"; '
                      f'RUN_STATUS=0; {setup}; cat "$EVIDENCE_DIR/{output}"')
            text = subprocess.run(["bash", "-c", script, "gate", str(directory)], cwd=ROOT, check=True, timeout=30,
                                  env={**os.environ, "ROOT": str(ROOT)}, capture_output=True, text=True).stdout
            self.assertEqual(text.count("guest_system_off="), 1, text)
            return "guest_system_off=true\n" in text
        self.assert_binds(reader)

    def test_runner_agent_shutdown_gate(self):
        self.runner_gate("SHUTDOWN_AFTER_AGENT_READY=1; write_agent_shutdown_gate", "agent-shutdown-gate.txt")

    def test_runner_agent_service_gate(self):
        self.runner_gate('AGENT_SERVICE_CONTROL="$1/app.ctl"; AGENT_SERVICE_COMMAND=whoami; write_agent_service_gate',
                         "agent-service-gate.txt")

    def test_runner_host_pause_resume_gate(self):
        observation = r"service_ready=true\nduring_state=T\nlog_stable_while_stopped=true\n" \
            r"continue_signal_sent=true\npost_resume_command_ok=true\n"
        self.runner_gate(f"HOST_PAUSE_RESUME_PROOF_MS=200; HOST_PAUSE_RESUME_CONTROL_STATUS=0; "
                         f"printf '{observation}' > \"$(host_pause_resume_observation_path)\"; "
                         "write_host_pause_resume_gate", "host-pause-resume-gate.txt")

    def test_nvme_performance_tiers(self):
        for name in ("run-hvf-nvme-performance-v1-tier.sh", "run-hvf-nvme-performance-v2-tier.sh"):
            lines = (LIVE_GATES / name).read_text(encoding="utf-8").splitlines()
            writeback = [index for index, line in enumerate(lines)
                         if line.startswith("grep -Eq '^NVMe (second namespace )?disk written back:' \"$BOOT/run.log\"")]
            self.assertEqual(len(writeback), 1, name)
            check = lines[writeback[0] - 1]
            self.assertIn('"$BOOT/run.log" || { INVALID_REASON=', check, name)

            def reader(directory: Path, check: str = check) -> bool:
                return subprocess.run(["bash", "-c", f'set -u; REPO="$1"; BOOT="$2"; INVALID_REASON=; {check}',
                                       "tier", str(ROOT), str(directory)], timeout=30).returncode == 0
            with self.subTest(name):
                self.assert_binds(reader)

    def test_snapshot_lifecycle_shutdown(self):
        def reader(directory: Path) -> bool:
            script = ('source "$1/scripts/snapshot-restore-lifecycle.sh"\nSTEP_TIMEOUT=2\n'
                      '(exit 0) & SNAPSHOT_LAUNCHER=$!\nsnapshot_shutdown "$2/ctl" "$2/run.log"\n')
            return subprocess.run(["bash", "-c", script, "contract", str(ROOT), str(directory)],
                                  capture_output=True, timeout=30).returncode == 0
        self.assert_binds(reader)

    def test_b7_lane_binds_stop_and_counters(self):
        lane, nonce = load("bridgevm_b7_lane", ROOT / "scripts/audio-teardown-result.py"), "a" * 64

        def reader(directory: Path) -> bool:
            result = directory / "playback-result.txt"
            result.write_text(f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
            try:
                return lane.validate(directory / "run.log", result, 0, nonce, 1)["pass"] is True
            except lane.AudioTeardownError:
                return False
        self.assert_binds(reader, audio=True)

    def test_t17_audio_counters_come_from_the_host_tail(self):
        def reader(directory: Path) -> bool:
            try:
                guest_evidence._audio((directory / "run.log").read_bytes())
                return True
            except ValueError:
                return False
        self.assertEqual(self.accepted(reader, {"genuine": GENUINE}), ["genuine"])
        self.assertEqual(self.accepted(reader, FORGED_AUDIO), [])

    def test_guest_input_shutdown_uses_the_shared_binding(self):
        source = (LIVE_GATES / "run-guest-input-live.py").read_text(encoding="utf-8")
        self.assertIn('receipt["clean_shutdown"] = guest_shutdown_observed(status, boot / "run.log")', source)
        self.assertNotIn("(system off)", source)
        self.assert_binds(lambda directory: guest_shutdown_observed(0, directory / "run.log"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
