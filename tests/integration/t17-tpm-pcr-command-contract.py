#!/usr/bin/env python3
"""The T17 TPM workload sends exactly TPM2_PCR_Read for PCR 0 and PCR 7 (SHA-256 bank),
returns only hashes, keeps CRLF, and is packaged and launched like the other T17
workloads without ever writing the shared t17-error file."""
from __future__ import annotations

import hashlib
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "scripts/win-assets"
HELPER, WORKLOAD = ASSETS / "bv-product-e2e-tpm.cs", ASSETS / "bv-product-e2e-tpm.ps1"
LAUNCHER = ASSETS / "bv-product-e2e-launch.ps1"
CONTRACT = ROOT / "tests/integration/t17-tpm-pcr-contract.ps1"
# TPM 2.0 Library Part 2 constants.
TPM_ST_NO_SESSIONS, TPM_CC_PCR_READ, TPM_ALG_SHA1, TPM_ALG_SHA256 = 0x8001, 0x0000017E, 0x0004, 0x000B
TPM_RC_SUCCESS, TPM_RC_INITIALIZE, TPM_RC_FAILURE = 0x000, 0x100, 0x101
PCR0, PCR7 = bytes(range(32)), bytes(range(0xA0, 0xC0))
BODY = ('$Body = "bridgevm-t17-tpm-pcr-v1`nnonce=$Nonce`nboot=$Boot`ntpm_present=$Present`ntpm_ready=$Ready'
        '`npcr_read=$Read`npcr0_sha256=$Pcr0`npcr7_sha256=$Pcr7`n"')
GUEST_CODES = ("ok", "tbs-context-failed", "tbs-submit-failed", "tpm-error-response",
               "pcr-selection-mismatch", "pcr-response-malformed", "helper-failed")


def selection(alg: int = TPM_ALG_SHA256, pcrs: tuple[int, ...] = (0, 7)) -> bytes:
    bitmap = bytearray(3)
    for pcr in pcrs:
        bitmap[pcr // 8] |= 1 << (pcr % 8)
    return struct.pack(">IHB", 1, alg, len(bitmap)) + bytes(bitmap)


def pcr_read_command() -> bytes:
    body = struct.pack(">I", TPM_CC_PCR_READ) + selection()
    return struct.pack(">HI", TPM_ST_NO_SESSIONS, 6 + len(body)) + body


def pcr_read_response(rc: int = TPM_RC_SUCCESS, chosen: bytes | None = None, digests=(PCR0, PCR7)) -> bytes:
    if rc != TPM_RC_SUCCESS:
        return struct.pack(">HII", TPM_ST_NO_SESSIONS, 10, rc)
    values = struct.pack(">I", len(digests)) + b"".join(struct.pack(">H", len(item)) + item for item in digests)
    body = struct.pack(">II", rc, 42) + (selection() if chosen is None else chosen) + values
    return struct.pack(">HI", TPM_ST_NO_SESSIONS, 6 + len(body)) + body


def csharp_command() -> bytes:
    match = re.search(r"Command = new byte\[\] \{(?P<body>[^}]*)\};", HELPER.read_text(encoding="utf-8"))
    if match is None:
        raise AssertionError("the helper's command literal is missing")
    return bytes(int(token, 16) for token in re.findall(r"0x([0-9A-Fa-f]{2})", match["body"]))


def text(path: Path) -> str:
    return path.read_bytes().decode("utf-8").replace("\r\n", "\n")


class TpmPcrCommandContract(unittest.TestCase):
    def test_command_is_the_documented_tpm2_pcr_read_of_pcr0_and_pcr7(self) -> None:
        self.assertEqual(pcr_read_command().hex(), "8001000000140000017e00000001000b03810000")
        self.assertEqual(csharp_command(), pcr_read_command())

    def test_guest_assets_keep_crlf(self) -> None:
        for path in (HELPER, WORKLOAD, LAUNCHER):
            data = path.read_bytes()
            with self.subTest(path=path.name):
                self.assertIn(b"\r\n", data)
                self.assertEqual(data.count(b"\n"), data.count(b"\r\n"))
                self.assertEqual(data.count(b"\r"), data.count(b"\r\n"))

    def test_workload_is_packaged_and_launched_like_the_other_t17_workloads(self) -> None:
        package = (ROOT / "apps/macos/scripts/package-hvf-control-app.sh").read_text(encoding="utf-8")
        self.assertTrue("  bv-product-e2e.ps1 bv-product-e2e-tpm.ps1 bv-product-e2e-tpm.cs \\\n" in package,
                        "the app package does not install both TPM guest assets")
        launcher = text(LAUNCHER)
        self.assertIn("'MarkerRestoredA', 'AgentResult', 'TpmBoot1', 'TpmBoot2', 'TpmBoot3')]", launcher)
        self.assertIn("$Workload = if ($Action -like 'TpmBoot?') { 'bv-product-e2e-tpm.ps1' } "
                      "else { 'bv-product-e2e.ps1' }", launcher)
        self.assertIn("'-File', \"C:\\bridgevm-share\\$Workload\",", launcher)
        self.assertEqual(launcher.count("Invoke-CimMethod -ClassName Win32_Process -MethodName Create"), 1)
        workload = text(WORKLOAD)
        self.assertIn("[ValidateSet('TpmBoot1', 'TpmBoot2', 'TpmBoot3')]", workload)
        self.assertIn("Add-Type -Path (Join-Path $Share 'bv-product-e2e-tpm.cs')", workload)

    def test_workload_writes_one_whole_bounded_result_and_never_a_t17_error(self) -> None:
        workload = text(WORKLOAD)
        self.assertFalse("t17-error" in workload, "the TPM workload names the shared T17 error file")
        self.assertEqual(workload.count(BODY), 1)
        self.assertEqual(workload.count("[IO.File]::WriteAllBytes($Staged,"), 1)
        self.assertIn('Move-Item -LiteralPath $Staged -Destination (Join-Path $Share "t17-tpm-boot$Boot-$Prefix.txt") -Force',
                      workload)
        self.assertLess(workload.index("[IO.File]::WriteAllBytes($Staged,"), workload.index("Move-Item -LiteralPath $Staged"))
        for code in GUEST_CODES[1:]:
            self.assertIn(f'"{code}"' if code != "helper-failed" else f"'{code}'",
                          HELPER.read_text(encoding="utf-8") + workload)
        longest = BODY.split('"')[1].replace("`n", "\n")
        for name, value in (("Nonce", "a" * 64), ("Boot", "3"), ("Present", "unknown"), ("Ready", "unknown"),
                            ("Read", max(GUEST_CODES, key=len)), ("Pcr0", "b" * 64), ("Pcr7", "c" * 64)):
            longest = longest.replace(f"${name}", value)
        self.assertLessEqual(len(longest.encode()), 512)

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_guest_parser_accepts_only_the_spec_built_success_response(self) -> None:
        def parse(response: bytes) -> list[str]:
            completed = subprocess.run(["pwsh", "-NoProfile", "-File", str(CONTRACT), "-ResponseHex", response.hex()],
                                       capture_output=True, text=True, timeout=120)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            return completed.stdout.split()

        self.assertEqual(parse(pcr_read_response()),
                         ["ok", hashlib.sha256(PCR0).hexdigest(), hashlib.sha256(PCR7).hexdigest()])
        for response, status in (
            (pcr_read_response(TPM_RC_FAILURE), "tpm-error-response"),
            (pcr_read_response(TPM_RC_INITIALIZE), "tpm-error-response"),
            (pcr_read_response(chosen=selection(pcrs=()), digests=()), "pcr-selection-mismatch"),
            (pcr_read_response(chosen=selection(TPM_ALG_SHA1)), "pcr-selection-mismatch"),
            (pcr_read_response(digests=(PCR0[:20], PCR7[:20])), "pcr-response-malformed"),
            (pcr_read_response()[:-1], "pcr-response-malformed"),
        ):
            with self.subTest(status=status, response=response.hex()):
                self.assertEqual(parse(response), [status, "absent", "absent"])

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_powershell_contract_passes(self) -> None:
        completed = subprocess.run(["pwsh", "-NoProfile", "-File", str(CONTRACT)],
                                   capture_output=True, text=True, timeout=180)
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("T17 TPM PCR contract: PASS", completed.stdout)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
