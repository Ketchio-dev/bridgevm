#!/usr/bin/env python3
"""Every NVMe write-back reader binds to the final host report's media records.

The probe prints `NVMe disk written back: PATH (N bytes)` as each write
completes, before the final report and in the same run.log as guest agent
output, so a guest can print the same line. final_report.rs repeats each
completed write as a `host media: ` record on the lines right after the
report's stop record. Each forged log carries a genuine SYSTEM_OFF report and
host teardown in which the host recorded no NVMe write-back, so only the author
of the write-back line differs. A current-format log still passes.
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
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
CLI = LIVE_GATES / "hvf_terminal_evidence.py"
SHELL = ROOT / "scripts/hvf-terminal-report.sh"
sys.path.insert(0, str(LIVE_GATES))
from hvf_stop_line import SYSTEM_OFF  # noqa: E402
from hvf_terminal_report import system_off_offset  # noqa: E402


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


READERS = load("hvf_stop_readers_contract", ROOT / "tests/integration/hvf-stop-readers-contract.py")
BANNER, AGENT, TEARDOWN = READERS.BANNER, READERS.AGENT, READERS.TEARDOWN
VARS = "UEFI vars written back: /tmp/vars.fd (67108864 bytes)"
WRITEBACK = "NVMe disk written back: /tmp/windows.raw (4096 bytes)"
EXITS = "exits: 1 (vtimer 0, psci 1, surplus-canceled 0), last PC: 0x0\nsymbol lines: 0\n"


def log(guest: str = "", unframed: str = f"{VARS}\n{WRITEBACK}\n",
        framed: str = f"host media: {VARS}\nhost media: {WRITEBACK}\n", serial: str = "UEFI firmware\r\n",
        stop: str = SYSTEM_OFF, body: str = EXITS) -> str:
    """A run.log in the runtime's layout: persistence lines, then the report with its media records."""
    size = len(serial.encode())
    return (f"{AGENT}{guest}{unframed}REGS: pc=0x0 lr=0x0\n{BANNER}\n{stop}\n{framed}{body}"
            f"serial raw bytes: {size} output bytes: {size}\n--- serial (tail) ---\n{serial}\n--- end ---\n{TEARDOWN}")


def agent(output: str) -> str:
    return f"BVAGENT CMD hostname exit=0\n{output}\nBVAGENT END hostname\n"


# The host persisted the vars but recorded no NVMe write-back.
NO_NVME = {"unframed": f"{VARS}\n", "framed": f"host media: {VARS}\n"}
GENUINE = log()
FORGED = {
    "agent command output": log(agent(WRITEBACK), **NO_NVME),
    "agent output after a carriage return": log(agent(f"bridgevm\\user\r{WRITEBACK}"), **NO_NVME),
    "serial tail": log(serial=f"boot\r\n{WRITEBACK}\r\n", **NO_NVME),
    "report fragment in the serial tail": log(serial=f"boot\n{BANNER}\n{SYSTEM_OFF}\nhost media: {WRITEBACK}\n",
                                              **NO_NVME),
    "report fragment in agent output": log(agent(f"{BANNER}\n{SYSTEM_OFF}\nhost media: {WRITEBACK}"), **NO_NVME),
    "record without a host report": AGENT + agent(WRITEBACK) + f"{VARS}\n" + TEARDOWN,
}
RECORD = f"host media: {WRITEBACK}\n"
# Grammar corners the Python binding, its CLI and the shell binding must decide alike.
ACCEPTED = {
    "genuine": GENUINE,
    "watchdog stop": log(stop="stop: watchdog (CANCELED)"),
    "only the NVMe record": log(framed=RECORD),
    "record right before the count": log(body=""),
    "escaped path": log(framed=RECORD.replace("windows.raw", "\\xed\\x95\\x9c.raw")),
    "all three namespaces": log(framed=f"host media: {VARS}\n{RECORD}host media: NVMe target namespace (NSID 2) "
                                       "written back: /tmp/target.raw (4096 bytes)\n"),
    # The first line that is not a whole record ends the run of records.
    "unknown record after the NVMe record": log(framed=f"{RECORD}host media: NVMe disk flushed: x (1 bytes)\n"),
}
REJECTED = {
    **FORGED,
    "record after the exits line": log(framed=f"host media: {VARS}\n", body=EXITS + RECORD),
    "two NVMe records": log(framed=RECORD * 2),
    "snapshot instead of write-back": log(framed=RECORD.replace("written back", "snapshot written")),
    "target namespace only": log(framed=RECORD.replace("NVMe disk", "NVMe target namespace (NSID 2)")),
    "record with a carriage return": log(framed=RECORD.replace("\n", "\r\n")),
    "record with a NUL": log(framed=RECORD.replace("windows", "win\x00dows")),
    "record with a raw non-ASCII path": log(framed=RECORD.replace("windows", "한")),
    "unknown record before the NVMe record": log(framed=f"host media: UEFI vars written: x (1 bytes)\n{RECORD}"),
    "record without its byte count": log(framed=f"host media: {WRITEBACK.split(' (')[0]}\n"),
    "stop record with a non-ASCII character": log(stop="stop: watchdog (CANCELED) é"),
    "legacy log without records": log(framed=""),
}
BEYOND_UNICODE = GENUINE.encode().replace(b"(system off)\n", b"(system off)\xf4\x90\x80\x80\n", 1)


class HostMediaReadersContract(unittest.TestCase):
    def accepted(self, reader, logs: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as temp:
            results = []
            for name, text in logs.items():
                directory = Path(temp) / str(len(results))
                directory.mkdir()
                (directory / "run.log").write_bytes(text.encode() if isinstance(text, str) else text)
                results.append((name, reader(directory)))
            return [name for name, passed in results if passed]

    def assert_binds(self, reader) -> None:
        self.assertEqual(self.accepted(reader, {"genuine": GENUINE}), ["genuine"])
        self.assertEqual(self.accepted(reader, FORGED), [])
        with tempfile.TemporaryDirectory() as temp:
            (Path(temp) / "genuine.log").write_bytes(GENUINE.encode())
            (Path(temp) / "run.log").symlink_to(Path(temp) / "genuine.log")
            self.assertFalse(reader(Path(temp)), "run.log linked to a genuine log")

    def status(self, command: list[str]) -> int:
        result = subprocess.run(command, capture_output=True, timeout=120)
        self.assertIn(result.returncode, (0, 1), result.stderr)
        return result.returncode

    def cli(self, log_path: Path) -> int:
        return self.status([sys.executable, str(CLI), "--require-nvme-write-back", str(log_path)])

    def shell(self, log_path: Path) -> int:
        return self.status(["/bin/bash", str(SHELL), "--require-nvme-write-back", str(log_path)])

    def test_bindings_decide_alike(self):
        """The packaged runner has no Python, so its shell binding is checked against the module."""
        from hvf_host_media import host_media_records, nvme_write_back_offset
        record = f"\nhost media: {WRITEBACK}".encode()
        edges = {f"framing: {name}": data.replace(SYSTEM_OFF.encode() + b"\n", SYSTEM_OFF.encode() + record + b"\n", 1)
                 for name, data in READERS.EDGES.items()}
        cases = {**{name: text.encode() for name, text in {**ACCEPTED, **REJECTED}.items()},
                 "stop record beyond U+10FFFF": BEYOND_UNICODE, **edges}
        decided = {name for name, data in cases.items() if nvme_write_back_offset(data) is not None}
        self.assertEqual(decided - set(edges), set(ACCEPTED))
        # A record right after a bound SYSTEM_OFF stop binds exactly when the stop does.
        self.assertEqual(decided & set(edges), {name for name, data in edges.items()
                                                 if system_off_offset(data) is not None})
        raw = GENUINE.encode()
        self.assertEqual(host_media_records(raw), [(raw.index(b"host media: UEFI"), f"host media: {VARS}"),
                                                   (raw.index(b"host media: NVMe"), f"host media: {WRITEBACK}")])
        self.assertEqual(nvme_write_back_offset(raw), raw.index(b"host media: NVMe"))
        self.assertIsNone(host_media_records(FORGED["record without a host report"].encode()))
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "run.log"
            for name, data in cases.items():
                path.write_bytes(data)
                expected = 0 if name in decided else 1
                self.assertEqual((self.cli(path), self.shell(path)), (expected, expected), name)
            self.assertEqual((self.cli(Path(temp) / "missing.log"), self.shell(Path(temp) / "missing.log")), (1, 1))
        for command in ([sys.executable, str(CLI), "--require-nvme-write-back"],
                        ["/bin/bash", str(SHELL), "--require-nvme-write-back"]):
            self.assertEqual(subprocess.run(command, capture_output=True, timeout=30).returncode, 2, command)

    def test_records_are_what_the_probe_prints(self):
        lines = [line.strip() for line in (PROBE / "final_report.rs").read_text(encoding="utf-8").splitlines()]
        stop = lines.index('println!("stop: {}", $stop_reason);')
        self.assertEqual(lines[stop - 1], 'println!("=== EDK2 boot probe (with Apple hv_gic) ===");')
        self.assertEqual(lines[stop + 1], 'for record in &host_media { println!("{record}"); }')
        persisted = lines.index("let host_media = persist_stop_media($platform, &$media, &mut $media_lease);")
        self.assertLess(persisted, stop)
        source = (PROBE / "stop_media.rs").read_text(encoding="utf-8")
        self.assertIn('pub(crate) const HOST_MEDIA_RECORD: &str = "host media: ";', source)
        self.assertIn('"{HOST_MEDIA_RECORD}{}: {} ({} bytes)",', source)
        self.assertIn("escape_guest_text(write.path.as_os_str().as_bytes())", source)
        # The grammar's subjects and kinds are the ones the probe labels its writes with.
        self.assertIn('report_media_writes("UEFI vars", &vars)', source)
        subjects = (PROBE / "storage_persistence.rs").read_text(encoding="utf-8")
        for literal in ('Self::Primary => "NVMe disk",', 'Self::Target => "NVMe target namespace (NSID 2)",'):
            self.assertIn(literal, subjects)
        kinds = (ROOT / "crates/bridgevm-hvf/src/media.rs").read_text(encoding="utf-8")
        for literal in ('Self::Snapshot => format!("{subject} snapshot written"),',
                        'Self::WriteBack => format!("{subject} written back"),'):
            self.assertIn(literal, kinds)

    def test_cli_and_shell_bind(self):
        self.assert_binds(lambda directory: self.cli(directory / "run.log") == 0)
        self.assert_binds(lambda directory: self.shell(directory / "run.log") == 0)

    def test_b7_lane(self):
        lane, nonce = load("bridgevm_b7_lane", ROOT / "scripts/audio-teardown-result.py"), "a" * 64

        def reader(directory: Path) -> bool:
            result = directory / "playback-result.txt"
            result.write_text(f"B7 PLAYBACK PASS nonce={nonce} wav_bytes=384044\n", encoding="utf-8")
            try:
                return lane.validate(directory / "run.log", result, 0, nonce, 1)["pass"] is True
            except lane.AudioTeardownError:
                return False
        self.assert_binds(reader)

    def test_nvme_performance_tiers(self):
        for name in ("run-hvf-nvme-performance-v1-tier.sh", "run-hvf-nvme-performance-v2-tier.sh"):
            lines = (LIVE_GATES / name).read_text(encoding="utf-8").splitlines()
            stop = [index for index, line in enumerate(lines) if '--require-system-off "$BOOT/run.log"' in line]
            self.assertEqual(len(stop), 1, name)
            check = lines[stop[0] + 1]
            self.assertIn('"$BOOT/run.log" || { INVALID_REASON=', check, name)

            def reader(directory: Path, check: str = check) -> bool:
                return subprocess.run(["bash", "-c", f'set -u; REPO="$1"; BOOT="$2"; INVALID_REASON=; {check}',
                                       "tier", str(ROOT), str(directory)], capture_output=True,
                                      timeout=30).returncode == 0
            with self.subTest(name):
                self.assert_binds(reader)
                self.assertEqual(sum("written back" in line for line in lines), 0, name)

    def runner_gate(self, setup: str, output: str) -> None:
        def reader(directory: Path) -> bool:
            script = ('set -euo pipefail; source scripts/run-hvf-windows-installed-boot-runner.sh; EVIDENCE_DIR="$1"; '
                      f'RUN_STATUS=0; {setup}; cat "$EVIDENCE_DIR/{output}"')
            text = subprocess.run(["bash", "-c", script, "gate", str(directory)], cwd=ROOT, check=True, timeout=30,
                                  env={**os.environ, "ROOT": str(ROOT)}, capture_output=True, text=True).stdout
            self.assertEqual(text.count("nvme_writeback="), 1, text)
            return "nvme_writeback=true\n" in text
        self.assert_binds(reader)

    def test_runner_agent_service_gate(self):
        self.runner_gate('AGENT_SERVICE_CONTROL="$1/app.ctl"; AGENT_SERVICE_COMMAND=whoami; write_agent_service_gate',
                         "agent-service-gate.txt")

    def test_runner_host_pause_resume_gate(self):
        observation = r"service_ready=true\nduring_state=T\nlog_stable_while_stopped=true\n" \
            r"continue_signal_sent=true\npost_resume_command_ok=true\n"
        self.runner_gate(f"HOST_PAUSE_RESUME_PROOF_MS=200; HOST_PAUSE_RESUME_CONTROL_STATUS=0; "
                         f"printf '{observation}' > \"$(host_pause_resume_observation_path)\"; "
                         "write_host_pause_resume_gate", "host-pause-resume-gate.txt")

    def test_runner_uses_the_shell_binding(self):
        runner = (ROOT / "scripts/run-hvf-windows-installed-boot-runner.sh").read_text(encoding="utf-8")
        self.assertEqual(runner.count('/bin/bash "$ROOT/scripts/hvf-terminal-report.sh" --require-nvme-write-back '
                                      '"$EVIDENCE_DIR/run.log" && nvme_writeback="true"'), 2)
        self.assertNotIn("written back", runner)

    def test_snapshot_lifecycle_shutdown(self):
        def reader(directory: Path) -> bool:
            script = ('source "$1/scripts/snapshot-restore-lifecycle.sh"\nSTEP_TIMEOUT=2\n'
                      '(exit 0) & SNAPSHOT_LAUNCHER=$!\nsnapshot_shutdown "$2/ctl" "$2/run.log"\n')
            return subprocess.run(["bash", "-c", script, "contract", str(ROOT), str(directory)],
                                  capture_output=True, timeout=30).returncode == 0
        self.assert_binds(reader)
        self.assertNotIn("written back", (ROOT / "scripts/snapshot-restore-lifecycle.sh").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
