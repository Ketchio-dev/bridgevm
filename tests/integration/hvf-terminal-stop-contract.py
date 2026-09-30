#!/usr/bin/env python3
"""A stop record is host evidence only as the final report's framed stop line.

Guest bytes reach run.log as agent command output before the final report and
as the counted serial tail inside it (final_report.rs), so a line equal to the
stop text says nothing about who wrote it. Each case puts the record where a
guest can and runs every acceptance consumer; the framing contract ties the
banner, count, tail and footer to what final_report.rs prints.
"""

from __future__ import annotations

import hashlib
import importlib
import importlib.util
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
SWIFT = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
import t17_terminal_report_tail as report_tail  # noqa: E402
import windows_product_e2e_guest_evidence as guest_evidence  # noqa: E402
from b9_raw_focus_order import verify_raw_focus_order  # noqa: E402
from hvf_guest_shutdown import guest_shutdown_observed  # noqa: E402
from hvf_stop_line import SYSTEM_OFF  # noqa: E402

BANNER = "=== EDK2 boot probe (with Apple hv_gic) ==="
DIAGNOSTIC = "stop: host diagnostic stop requested"
RECREATE = "stop: PSCI 0x84000009 exiting for process recreation (exit 42)"
READY = "BVAGENT READY host=BRIDGEVM t=20075\n"
TEARDOWN = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\nhda CoreAudio stats: " + " ".join(  # B7 fields, three typed shutdown statuses
    f"{k}={48000 if k == 'frames_rendered' else 3 if k.startswith(('callback_errors', 'callback_stopping_errors', 'callback_expected', 'callback_stopping_enqueue')) else 0}" for k in guest_evidence.t17_audio_counters._B7.STAT_FIELDS) + "\n"
VIRGL = "Sep  1 12:57:02  virgl_render_server[43160] <Debug>: socket disconnected\n"
NESTED = f"boot\r\n{BANNER}\n{SYSTEM_OFF}\nserial raw bytes: 0 output bytes: 0\n--- serial (tail) ---\n"


def report(stop: str, serial: str = "UEFI firmware\r\n", tail: str = "") -> str:
    size = len(serial.encode())
    return (f"REGS: pc=0x0 lr=0x0\n{BANNER}\n{stop}\nexits: 1 (vtimer 0, psci 1, surplus-canceled 0), last PC: 0x0\n"
            f"symbol lines: 0\nserial raw bytes: {size} output bytes: {size}\n"
            f"--- serial (tail) ---\n{serial}\n--- end ---\n{tail}")


def agent(command: str, output: str) -> str:
    return f"BVAGENT CMD {command} exit=0\n{output}\nBVAGENT END {command}\n"


FORGED = {
    "serial tail after a non-terminal stop": READY + report(DIAGNOSTIC, f"Boot Manager\r\n{SYSTEM_OFF}\r\n", TEARDOWN),
    "report nested in the serial tail": READY + report(DIAGNOSTIC, NESTED, TEARDOWN),
    "nested report beside the real one": READY + report(SYSTEM_OFF, NESTED, TEARDOWN),
    "agent command output": READY + agent("whoami", SYSTEM_OFF) + report(DIAGNOSTIC, tail=TEARDOWN),
    "complete report in agent output": READY + agent("whoami", report(SYSTEM_OFF, "")) + report(DIAGNOSTIC, tail=TEARDOWN),
    "record without a host report": READY + agent("shutdown.exe /s /t 0 /f", SYSTEM_OFF) + TEARDOWN,
    "record after the teardown": READY + report(SYSTEM_OFF, tail=TEARDOWN + "BVAGENT SERVICE alive t=1\n"),
    "truncated footer": READY + TEARDOWN + report(SYSTEM_OFF)[:-len("--- end ---\n")],
}
SPLIT = READY + agent("whoami", f"bridgevm\\bridge\r{SYSTEM_OFF}") + report(DIAGNOSTIC, tail=TEARDOWN)
GENUINE = {
    "footer at end of log": READY + report(SYSTEM_OFF),
    "teardown after the footer": READY + agent("shutdown.exe /s /t 0 /f", "")
    + report(SYSTEM_OFF, "Windows Boot Manager\r\n\x1b[2J\r\n", TEARDOWN),
    "recreated generation first": report(RECREATE, tail=TEARDOWN) + "Guest RAM: 6144 MiB\n" + READY
    + report(SYSTEM_OFF, tail=TEARDOWN + VIRGL),
    "guest copy inside the tail": READY + report(SYSTEM_OFF, f"boot\r\n{SYSTEM_OFF}\r\n", TEARDOWN),
    "guest copy before the report": READY + agent("whoami", SYSTEM_OFF) + report(SYSTEM_OFF, tail=TEARDOWN),
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def final_stop(text: str) -> int:
    return text.encode().rindex(f"{BANNER}\n{SYSTEM_OFF}\nexits: ".encode()) + len(BANNER) + 1


def last_record(raw: bytes) -> int:
    """The offset the text-equality T17 capture picks: the last equal LF record."""
    return [match.start() for match in re.finditer(
        rb"(?<![^\n])" + re.escape(SYSTEM_OFF.encode()) + rb"(?=\r?\n)", raw)][-1]


class TerminalStopContract(unittest.TestCase):
    def test_framing_is_what_final_report_prints(self):
        source = (PROBE / "final_report.rs").read_text(encoding="utf-8")
        lines = [line.strip() for line in source.splitlines()]
        banner = lines.index(f'println!("{BANNER}");')
        self.assertEqual(lines[banner + 1], 'println!("stop: {}", $stop_reason);')
        count = lines.index('println!("serial raw bytes: {} output bytes: {}", serial.len(), rendered.len());')
        self.assertEqual(lines[count - 1], "let rendered = String::from_utf8_lossy(&serial);")
        self.assertEqual(lines[count + 1:count + 4],
                         ['println!("--- serial (tail) ---\\n{rendered}\\n--- end ---");', "}};", "}"])
        self.assertEqual((source.count(BANNER), source.count("--- serial (tail) ---")), (1, 1))
        # Between banner and count the only guest-derived records are these prefixed symbol lines.
        self.assertIn('.filter(|line| line.starts_with("add-symbol-file "))',
                      (PROBE / "boot_telemetry.rs").read_text(encoding="utf-8"))
        runtime = (PROBE / "probe_runtime.rs").read_text(encoding="utf-8")
        self.assertEqual(runtime.count("persist_and_report_stop!("), 1)
        self.assertRegex(runtime, r"persist_and_report_stop!\([^;]*\);\s*break 'reboot;")
        self.assertEqual((report_tail.BANNER, report_tail.SERIAL, report_tail.FOOTER),
                         (f"\n{BANNER}\n".encode(), b"\n--- serial (tail) ---\n", b"\n--- end ---\n"))

    def test_consumers_share_one_final_report_binding(self):
        self.assertIsNotNone(importlib.util.find_spec("hvf_terminal_report"), "hvf_terminal_report.py is missing")
        binding = importlib.import_module("hvf_terminal_report")
        self.assertEqual((binding.BANNER, binding.SERIAL, binding.FOOTER, binding.COUNT_NEW),
                         (report_tail.BANNER, report_tail.SERIAL, report_tail.FOOTER, report_tail.COUNT_NEW))
        swift = {path.name: path.read_text(encoding="utf-8") for path in SWIFT.glob("*.swift")}
        self.assertIn("HvfTerminalReport.swift", swift)
        for literal in (f"\\n{BANNER}\\n", "\\n--- serial (tail) ---\\n", "\\n--- end ---\\n"):
            self.assertIn(f'Data("{literal}".utf8)', swift["HvfTerminalReport.swift"])
        self.assertIn("HvfTerminalReport.stop(in: data)", swift["T17GuestProof.swift"])
        self.assertNotIn("lines.last(where: { $0.line == HvfStopLine.systemOff })", swift["T17GuestProof.swift"])
        self.assertIn("HvfTerminalReport.stop(in:", swift["HvfStopLine.swift"])
        self.assertNotIn("isNewline", swift["HvfStopLine.swift"])
        for name in ("T17ProductRunner.swift", "A9ImportProductRunner.swift"):
            self.assertIn("HvfStopLine.systemOffObserved(in:", swift[name], name)
            self.assertNotIn("contains(HvfStopLine.systemOff)", swift[name], name)
        for name in ("windows_product_e2e_guest_evidence.py", "b9_raw_focus_order.py", "hvf_guest_shutdown.py"):
            self.assertIn("system_off_offset(", (LIVE_GATES / name).read_text(encoding="utf-8"), name)

    def test_terminal_stop_is_the_final_host_record(self):
        binding = importlib.import_module("hvf_terminal_report")
        for name, text in GENUINE.items():
            self.assertEqual(binding.terminal_stop(text.encode()), (final_stop(text), SYSTEM_OFF), name)
            self.assertEqual(binding.system_off_offset(text.encode()), final_stop(text), name)
        for name, text in {**FORGED, "record split by a carriage return": SPLIT}.items():
            stop = binding.terminal_stop(text.encode())
            self.assertTrue(stop is None or stop[1] != SYSTEM_OFF, name)
            self.assertIsNone(binding.system_off_offset(text.encode()), name)
        self.assertEqual(binding.terminal_stop(report(RECREATE, tail=TEARDOWN).encode())[1], RECREATE)

    def test_guest_shutdown_needs_the_final_report(self):
        with tempfile.TemporaryDirectory() as temp:
            log = Path(temp) / "run.log"
            for name, text in {**GENUINE, **FORGED, "record split by a carriage return": SPLIT}.items():
                log.write_bytes(text.encode())
                self.assertEqual(guest_shutdown_observed(0, log), name in GENUINE, name)
                self.assertFalse(guest_shutdown_observed(1, log), name)
            self.assertFalse(guest_shutdown_observed(0, Path(temp) / "missing.log"))

    def test_b9_raw_order_needs_the_final_report_stop(self):
        nonce, hwnd, control, script, umd = "f" * 32, 777, "a" * 64, "b" * 64, "d" * 64
        base = (r"powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMB9\bv-b9-control.ps1"
                + f" -Action {{}} -ExpectedControlSha256 {control}")
        launch = base.format("Launch") + f" -Nonce {nonce} -ExpectedGuestScriptSha256 {script} -ExpectedD3D11UmdSha {umd}"
        query = base.format("Foreground") + f" -Hwnd {hwnd}"
        focus = f"BVAGENT WINFOCUS {hwnd} -> OK WINFOCUS\n" + agent(query, f"B9-FOREGROUND-{hwnd}")
        events = agent(launch, f"B9-WORKLOAD-LAUNCHED-{nonce}") + focus * 2 + "live input accepted: command=Key(<redacted>)\n"
        verify_raw_focus_order((events + report(SYSTEM_OFF, tail=TEARDOWN + VIRGL)).encode(),
                               nonce, hwnd, control, script, umd)
        for name, tail in (("record without a host report", agent("shutdown.exe /s /t 0 /f", SYSTEM_OFF) + TEARDOWN),
                           ("record after the teardown", report(SYSTEM_OFF, tail=TEARDOWN + "BVAGENT SERVICE alive t=1\n")),
                           ("truncated footer", TEARDOWN + report(SYSTEM_OFF)[:-len("--- end ---\n")])):
            with self.assertRaisesRegex(ValueError, "order differs", msg=name):
                verify_raw_focus_order((events + tail).encode(), nonce, hwnd, control, script, umd)

    def test_t17_guest_evidence_binds_shutdown_to_the_final_report(self):
        self.verify_t17(GENUINE["teardown after the footer"])
        for name, text in FORGED.items():
            with self.assertRaisesRegex(ValueError, "first_shutdown_offset is invalid", msg=name):
                self.verify_t17(text)

    def verify_t17(self, first: str) -> None:
        """Offsets are the ones the text-equality capture records for each log."""
        nonce = "e" * 64
        prefix, final = nonce[:12], GENUINE["teardown after the footer"]
        with tempfile.TemporaryDirectory() as temp:
            bundle, share = Path(temp) / "bundle.vmbridge", Path(temp) / "share"
            evidence_root = bundle / "metadata/product-e2e"
            for directory in (bundle / "logs/hvf", evidence_root, share):
                directory.mkdir(parents=True)
            identity = {"job_id": "t17-fixture", "commit": "c" * 40, "lane": 1, "nonce": nonce, "vm_slug": "vm-1"}
            clipboard = f"브리지VM T17 클립보드 왕복 v1\n{nonce}\n".encode()
            (share / f"t17-clipboard-host-{prefix}.txt").write_bytes(clipboard)
            observations: dict[str, object] = {"audio_playback_count": 1, "audio_error_count": 0}
            for key, stem, tag in (("keyboard_pointer_challenge", "keyboard-pointer", "keyboard-pointer"),
                                   ("clipboard_roundtrip", "clipboard-guest", ""), ("share_host_to_guest", "", "share"),
                                   ("share_guest_to_host", "guest", "guest-share"),
                                   ("network_result", "network", "network-ok"), ("audio_result", "audio", "audio-ok"),
                                   ("snapshot_marker_a", "snapshot-a", "snapshot-a"),
                                   ("snapshot_marker_b", "snapshot-b", "snapshot-b"),
                                   ("snapshot_marker_restored_a", "snapshot-restored-a", "snapshot-a")):
                body = f"bridgevm-t17-{tag}-v1\n{nonce}\n".encode() if tag else clipboard
                (share / (f"t17-{stem}-{prefix}.txt" if stem else f"t17-{prefix}.txt")).write_bytes(body)
                observations[f"{key}_sha256"] = digest(body)
            agent_raw = json.dumps({"schema_version": "bridgevm.windows-product-e2e-agent-result.v2",
                                    **identity, **observations}).encode()
            (share / f"t17-agent-result-{prefix}.json").write_bytes(agent_raw)
            (evidence_root / "agent-result.json").write_bytes(agent_raw)
            evidence = {"schema_version": "bridgevm.windows-product-e2e-guest-evidence.v2", **identity,
                        **observations, "agent_result_sha256": digest(agent_raw)}
            for name, path, text, ready, stop in (
                    ("first", evidence_root / "first-run.log", first, "first-ready", "first-shutdown"),
                    ("mutation", evidence_root / "mutation-run.log", final, "mutation-ready", "mutation-shutdown"),
                    ("final", bundle / "logs/hvf/run.log", final, "final-ready", "second-shutdown")):
                raw = text.encode()
                path.write_bytes(raw)
                evidence[f"{name}_run_log_sha256"] = digest(raw)
                for event, offset in ((ready, raw.index(b"BVAGENT READY")), (stop, last_record(raw))):
                    line = raw[offset:raw.index(b"\n", offset)].rstrip(b"\r").decode()
                    field = event.replace("-", "_")
                    evidence[f"{field}_offset"] = offset
                    evidence[f"{field}_line_nonce_sha256"] = digest(f"bridgevm-t17-{event}-v1\n{nonce}\n{line}\n".encode())
            evidence_path = bundle / "metadata/guest-evidence.json"
            evidence_path.write_bytes(json.dumps(evidence).encode())
            guest_evidence.verify({"disk_path": str(bundle / "disks/hvf-target.raw"), "share_path": str(share),
                                   "guest_evidence_path": str(evidence_path), **identity})


if __name__ == "__main__":
    unittest.main(verbosity=2)
