#!/usr/bin/env python3
"""The T19 BitLocker guest workload adds only a TPM protector and starts a used-space
conversion through Win32_EncryptableVolume, never names a recovery password or key
package path, writes one bounded 12-line result that an independent parser accepts,
keeps CRLF, and is packaged, launched and contract-checked like the other product
E2E guest assets (docs/history/windows-hvf/bitlocker-restart-check-20261023.md)."""
from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "scripts/win-assets"
WORKLOAD, LAUNCHER = ASSETS / "bv-product-e2e-bitlocker.ps1", ASSETS / "bv-product-e2e-bitlocker-launch.ps1"
CONTRACT = ROOT / "tests/integration/t19-bitlocker-guest-contract.ps1"
RUNNER = ROOT / "tests/integration/windows-guest-pwsh-contracts.ps1"
WORKFLOW = ROOT / ".github/workflows/t17-tpm-pcr-contract.yml"
ALLOWED = {"GetConversionStatus", "GetProtectionStatus", "GetLockStatus", "GetKeyProtectors", "GetKeyProtectorType",
           "GetKeyProtectorPlatformValidationProfile", "ProtectKeyWithTPM", "Encrypt"}
DENIED = (r"NumericalPassword", r"KeyPackage", r"RecoveryPassword", r"RecoveryInformation", r"ExternalKey",
          r"Passphrase", r"Get-BitLockerVolume", r"Enable-BitLocker", r"BitLockerKeyProtector", r"Suspend-BitLocker",
          r"manage-bde", r"\bDecrypt\b", r"DisableKeyProtectors", r"EncryptAfterHardwareTest", r"Get-WmiObject",
          r"Invoke-WmiMethod", r"\[wmi(class)?\]", r"t17-error")
OUTCOMES = ("ok", "edition-unsupported", "tpm-not-ready", "bitlocker-unavailable", "volume-not-decrypted",
            "protectors-present", "protector-add-failed", "protector-unexpected", "encrypt-failed", "query-failed",
            "script-failed")
PCR_READS = ("ok", "tbs-context-failed", "tbs-submit-failed", "tpm-error-response", "pcr-selection-mismatch",
             "pcr-response-malformed", "helper-failed")


def text(path: Path) -> str:
    return path.read_bytes().decode("utf-8").replace("\r\n", "\n")


def function_body(source: str, name: str) -> str:
    start = source.index(f"function {name}")
    following = source.find("\nfunction ", start + 1)
    return source[start:following if following >= 0 else len(source)]


def ascending(value: str, maximum: int) -> bool:
    if value in ("none", "unknown"):
        return True
    if not re.fullmatch(r"(0|[1-9][0-9]?)(,(0|[1-9][0-9]?))*", value):
        return False
    numbers = [int(item) for item in value.split(",")]
    return numbers == sorted(set(numbers)) and numbers[-1] <= maximum


def parse(body: str) -> dict[str, str]:
    """An independent reading of the documented result grammar; raises on any other shape."""
    if len(body.encode()) > 512 or not body.endswith("\n") or "\r" in body or not body.isascii():
        raise ValueError("result is unbounded, unterminated or not plain ASCII")
    lines = body[:-1].split("\n")
    keys = ("nonce", "phase", "outcome", "edition", "volume", "protectors", "pcr_profile", "fve_code", "pcr_read",
            "pcr0_sha256", "pcr7_sha256")
    if len(lines) != 12 or lines[0] != "bridgevm-t19-bitlocker-v1":
        raise ValueError("result is not twelve lines of version 1")
    fields = {}
    for key, line in zip(keys, lines[1:]):
        if not line.startswith(key + "="):
            raise ValueError(f"line {key} is missing or out of order")
        fields[key] = line[len(key) + 1:]
    volume = re.fullmatch(r"([0-5]),([0-2]),([01]),(100|[1-9]?[0-9])", fields["volume"])
    checks = (re.fullmatch(r"[0-9a-f]{64}", fields["nonce"]), fields["phase"] in ("enable", "status"),
              fields["outcome"] in OUTCOMES,
              fields["edition"] in ("home", "pro", "enterprise", "education", "server", "other", "unknown"),
              volume or fields["volume"] == "unknown", ascending(fields["protectors"], 10),
              ascending(fields["pcr_profile"], 23),
              fields["fve_code"] == "none" or (re.fullmatch(r"0|[1-9][0-9]{0,4}", fields["fve_code"]) and int(fields["fve_code"]) <= 65535),
              fields["pcr_read"] in PCR_READS,
              all(re.fullmatch(r"[0-9a-f]{64}|absent", fields[key]) for key in ("pcr0_sha256", "pcr7_sha256")),
              (fields["pcr_read"] == "ok") == (fields["pcr0_sha256"] != "absent") == (fields["pcr7_sha256"] != "absent"))
    if not all(checks):
        raise ValueError(f"a field is outside its set: {fields}")
    return fields


class BitLockerGuestContract(unittest.TestCase):
    def test_guest_assets_keep_crlf(self) -> None:
        for path in (WORKLOAD, LAUNCHER):
            data = path.read_bytes()
            with self.subTest(path=path.name):
                self.assertIn(b"\r\n", data)
                self.assertEqual(data.count(b"\n"), data.count(b"\r\n"))
                self.assertEqual(data.count(b"\r"), data.count(b"\r\n"))

    def test_launcher_starts_only_the_bitlocker_workload_through_win32_process_create(self) -> None:
        launcher = text(LAUNCHER)
        self.assertIn("[ValidateSet('BitLockerEnable', 'BitLockerStatus')]", launcher)
        self.assertIn("[ValidatePattern('^[0-9a-f]{64}$')]", launcher)
        self.assertEqual(launcher.count("Invoke-CimMethod -ClassName Win32_Process -MethodName Create"), 1)
        self.assertIn("'-File', 'C:\\bridgevm-share\\bv-product-e2e-bitlocker.ps1', '-Action', $Action, '-Nonce', $Nonce", launcher)
        self.assertIn('Write-Output ("T19-BITLOCKER-LAUNCHED-$Action-$Prefix pid=" + $Result.ProcessId)', launcher)
        self.assertIn("[ValidateSet('BitLockerEnable', 'BitLockerStatus')]", text(WORKLOAD))

    def test_workload_calls_only_the_allowed_volume_methods_and_changes_the_volume_only_when_enabling(self) -> None:
        workload = text(WORKLOAD)
        called = re.findall(r"Invoke-BvMethod \$Volume '([A-Za-z]+)'", workload)
        self.assertEqual(set(called), ALLOWED)
        self.assertEqual(workload.count("Invoke-CimMethod"), 2, "only Invoke-BvMethod reaches CIM methods")
        self.assertEqual(function_body(workload, "Invoke-BvMethod").count("Invoke-CimMethod -InputObject $Volume -MethodName $Method"), 2)
        enable = function_body(workload, "Invoke-BvEnable")
        for method in ("ProtectKeyWithTPM", "Encrypt"):
            self.assertEqual(called.count(method), 1, method)
            self.assertEqual(enable.count(f"Invoke-BvMethod $Volume '{method}'"), 1, method)
        self.assertIn("Invoke-BvMethod $Volume 'Encrypt' @{ EncryptionFlags = [uint32]1 }", enable)
        self.assertNotIn("PlatformValidationProfile =", workload, "the protector uses the Windows default profile")
        self.assertLess(enable.index("'ProtectKeyWithTPM'"), enable.index("'Encrypt'"))

    def test_workload_never_names_a_recovery_or_key_package_path(self) -> None:
        for path in (WORKLOAD, LAUNCHER):
            source = text(path)
            for pattern in DENIED:
                with self.subTest(path=path.name, pattern=pattern):
                    self.assertIsNone(re.search(pattern, source, re.IGNORECASE))

    def test_workload_writes_one_whole_bounded_result(self) -> None:
        workload = text(WORKLOAD)
        template = [line for line in workload.split("\n") if line.strip().startswith('"bridgevm-t19-bitlocker-v1`n')]
        self.assertEqual(len(template), 1)
        self.assertEqual(template[0].count("`n"), 12)
        self.assertEqual(workload.count("[IO.File]::WriteAllBytes($Staged,"), 1)
        rename = 'Move-Item -LiteralPath $Staged -Destination (Join-Path $Share "t19-bitlocker-$Phase-$Prefix.txt") -Force'
        self.assertEqual(workload.count(rename), 1)
        self.assertLess(workload.index("[IO.File]::WriteAllBytes($Staged,"), workload.index(rename))
        self.assertIn("$Staged = \"C:\\ProgramData\\BridgeVM\\t19-bitlocker-$Phase.tmp\"", workload)
        self.assertIn("Add-Type -Path (Join-Path $Share 'bv-product-e2e-tpm.cs')", workload)
        self.assertIn("$Result = New-BvResult; $Result.Outcome = 'script-failed'", workload)

    def test_assets_are_packaged_beside_the_other_product_e2e_assets(self) -> None:
        package = (ROOT / "apps/macos/scripts/package-hvf-control-app.sh").read_text(encoding="utf-8")
        block = package[package.index("for asset in \\\n  bv-ppsspp-title.json"):]
        names = block[:block.index("\ndo\n")].replace("\\\n", " ").split()
        for name in ("bv-product-e2e-bitlocker-launch.ps1", "bv-product-e2e-bitlocker.ps1", "bv-product-e2e-tpm.cs"):
            self.assertEqual(names.count(name), 1, name)

    def test_the_existing_windows_job_runs_both_guest_contracts(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("        shell: powershell\n        run: tests/integration/windows-guest-pwsh-contracts.ps1\n", workflow)
        self.assertIn("actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1", workflow)
        runner = RUNNER.read_text(encoding="utf-8")
        self.assertIn("foreach ($Contract in 't17-tpm-pcr-contract.ps1', 't19-bitlocker-guest-contract.ps1') {", runner)
        self.assertIn("if ($LASTEXITCODE -ne 0) { throw", runner)

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_powershell_contract_passes(self) -> None:
        completed = subprocess.run(["pwsh", "-NoProfile", "-File", str(CONTRACT)], capture_output=True, text=True, timeout=180)
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        self.assertIn("T19 BitLocker guest contract: PASS", completed.stdout)

    @unittest.skipUnless(shutil.which("pwsh"), "pwsh is not installed")
    def test_every_scenario_result_matches_the_independent_grammar(self) -> None:
        completed = subprocess.run(["pwsh", "-NoProfile", "-File", str(CONTRACT), "-Emit"], capture_output=True, text=True, timeout=180)
        self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
        results = {item["name"]: item["body"] for item in map(json.loads, (line for line in completed.stdout.splitlines() if line.startswith("{")))}
        self.assertGreaterEqual(len(results), 20)
        parsed = {name: parse(body) for name, body in results.items()}
        self.assertEqual((parsed["enabled"]["outcome"], parsed["enabled"]["protectors"], parsed["enabled"]["volume"]), ("ok", "1", "2,0,0,0"))
        self.assertEqual(parsed["home edition"]["outcome"], "edition-unsupported")
        self.assertEqual(parsed["encryption refused"]["fve_code"], "161")
        self.assertEqual(parsed["unlocked under conversion"]["pcr_profile"], "0,2,4,11")
        self.assertEqual(len(results["longest"].encode()), max(len(body.encode()) for body in results.values()))
        for broken in (results["enabled"].replace("protectors=1", "protectors=1,1"), results["enabled"].replace("\n", "\r\n"),
                       results["enabled"] + "extra=1\n", results["enabled"].replace("phase=enable", "phase=both")):
            with self.assertRaises(ValueError):
                parse(broken)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
