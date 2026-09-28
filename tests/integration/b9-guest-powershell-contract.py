#!/usr/bin/env python3
"""Execute B9 guest input refusals in hosted Windows PowerShell without a VM."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
CHILD = REPO / "scripts/win-assets/bv-b9-vlc-playback.ps1"
HELPER = REPO / "scripts/win-assets/bv-b9-private-inputs.ps1"
GUEST_WORK = Path("C:/BridgeVM")
GUEST_SHARE = Path("C:/BridgeVMB9")
ZERO_SHA = "0" * 64

CALL_HELPER = r"""param(
    [Parameter(Mandatory=$true)][string]$Helper,
    [Parameter(Mandatory=$true)][string]$Source,
    [Parameter(Mandatory=$true)][string]$Target,
    [Parameter(Mandatory=$true)][string]$Expected,
    [Parameter(Mandatory=$true)][string]$Case
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. $Helper
if ($Case -eq 'mutate-private' -or $Case -eq 'truncate-private') {
    $script:TargetPath = $Target
    $script:Mutation = $Case
    $script:Mutated = $false
    $script:OriginalAssertHash = (Get-Command Assert-Hash -CommandType Function).ScriptBlock
    function Assert-Hash([string]$Path, [string]$Expected) {
        if ($Path -ceq $script:TargetPath -and -not $script:Mutated) {
            $script:Mutated = $true
            if ($script:Mutation -eq 'mutate-private') {
                $bytes = [IO.File]::ReadAllBytes($Path)
                $bytes[0] = $bytes[0] -bxor 1
                [IO.File]::WriteAllBytes($Path, $bytes)
            } else {
                $file = [IO.File]::Open($Path, [IO.FileMode]::Open,
                    [IO.FileAccess]::Write, [IO.FileShare]::None)
                try { $file.SetLength($file.Length - 1) } finally { $file.Dispose() }
            }
        }
        & $script:OriginalAssertHash $Path $Expected
    }
}
$observed = Copy-B9PrivateInput $Source $Target $Expected
Write-Output $observed
"""


def committed_asset(path: Path) -> bytes:
    relative = path.relative_to(REPO).as_posix()
    raw = path.read_bytes()
    if not raw or raw.count(b"\n") != raw.count(b"\r\n"):
        raise AssertionError(f"guest asset is not CRLF: {relative}")
    committed = subprocess.check_output(["git", "-C", str(REPO), "show", "HEAD:" + relative],
                                        timeout=20)
    if raw != committed:
        raise AssertionError(f"guest asset differs from exact HEAD: {relative}")
    return raw


class GuestPowerShellContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if os.name != "nt":
            raise RuntimeError("B9 guest PowerShell contracts require hosted Windows")
        child = committed_asset(CHILD)
        helper = committed_asset(HELPER)
        cls.child_sha = hashlib.sha256(child).hexdigest()
        helper_sha = hashlib.sha256(helper).hexdigest()
        if f"$helperHash = '{helper_sha}'".encode() not in child:
            raise AssertionError("child helper pin differs from exact HEAD")

    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="b9-guest-powershell-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.wrapper = self.root / "call-private-input.ps1"
        self.wrapper.write_text(CALL_HELPER, encoding="utf-8")
        self.nonce = secrets.token_hex(16)

    def powershell(self, script: Path, *args: str) -> subprocess.CompletedProcess[str]:
        env = {k: v for k, v in os.environ.items() if k.upper() != "PSMODULEPATH"}
        return subprocess.run(
            ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive",
             "-ExecutionPolicy", "Bypass", "-File", str(script), *map(str, args)],
            capture_output=True, text=True, timeout=20, check=False, env=env)

    def child(self, *args: str) -> subprocess.CompletedProcess[str]:
        return self.powershell(CHILD, "-Nonce", self.nonce,
                               "-ExpectedD3D11UmdSha", ZERO_SHA, *args)

    def helper(self, source: Path, target: Path, expected: str, case: str = "plain"):
        return self.powershell(self.wrapper, "-Helper", str(HELPER),
                               "-Source", str(source), "-Target", str(target),
                               "-Expected", expected, "-Case", case)

    def refused(self, result: subprocess.CompletedProcess[str], detail: str) -> None:
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(detail, result.stdout + result.stderr)

    def test_child_refuses_missing_and_wrong_self_hash_before_side_effects(self) -> None:
        for supplied in ((), ("-ExpectedGuestScriptSha256", ZERO_SHA)):
            with self.subTest(supplied=supplied):
                result = self.child(*supplied)
                self.refused(result, "ExpectedGuestScriptSha256" if not supplied else "B9 playback script differs from sealed source")
                self.assertFalse((GUEST_WORK / ("b9-work-" + self.nonce)).exists())
                for name in ("ready-", "collector-", "finished-"):
                    self.assertFalse((GUEST_SHARE / (name + self.nonce + ".json")).exists())

    def test_private_copy_accepts_intact_synthetic_bytes(self) -> None:
        source, target = self.root / "media.bin", self.root / "private.bin"
        raw = b"B9 synthetic private input\r\n"
        source.write_bytes(raw)
        expected = hashlib.sha256(raw).hexdigest()
        result = self.helper(source, target, expected)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((result.stdout.strip(), target.read_bytes()), (expected, raw))

    def test_private_copy_refuses_changed_empty_and_oversized_source(self) -> None:
        raw = b"B9 synthetic private input\r\n"
        expected = hashlib.sha256(raw).hexdigest()
        cases = ((b"X" + raw[1:], "B9 source hash mismatch"),
                 (b"", "B9 shared input type or size differs"),
                 (b"Z" * 7_500_001, "B9 shared input type or size differs"))
        for index, (content, detail) in enumerate(cases):
            with self.subTest(index=index):
                source = self.root / f"source-{index}.bin"
                target = self.root / f"private-{index}.bin"
                source.write_bytes(content)
                self.refused(self.helper(source, target, expected), detail)
                self.assertFalse(target.exists())

    def test_private_copy_refuses_changed_or_truncated_private_target(self) -> None:
        source = self.root / "source.bin"
        raw = b"B9 synthetic private input\r\n"
        source.write_bytes(raw)
        expected = hashlib.sha256(raw).hexdigest()
        for case in ("mutate-private", "truncate-private"):
            with self.subTest(case=case):
                target = self.root / (case + ".bin")
                self.refused(self.helper(source, target, expected, case),
                             "B9 source hash mismatch")
                self.assertTrue(target.is_file())
                self.assertNotEqual(target.read_bytes(), raw)

    def test_private_copy_refuses_existing_aliases_and_reparse_source(self) -> None:
        source = self.root / "source.bin"
        raw = b"B9 synthetic private input\r\n"
        source.write_bytes(raw)
        expected = hashlib.sha256(raw).hexdigest()
        hardlink = self.root / "hardlink.bin"
        os.link(source, hardlink)
        self.refused(self.helper(source, hardlink, expected),
                     "B9 private input already exists")
        self.assertEqual(hardlink.read_bytes(), raw)
        symlink = self.root / "symlink.bin"
        os.symlink(source, symlink)
        self.refused(self.helper(source, symlink, expected),
                     "B9 private input already exists")
        self.assertEqual(source.read_bytes(), raw)
        target = self.root / "from-reparse.bin"
        self.refused(self.helper(symlink, target, expected),
                     "B9 shared input type or size differs")
        self.assertFalse(target.exists())

    def test_child_refuses_tampered_shared_helper_before_dot_source(self) -> None:
        for path in (GUEST_WORK, GUEST_SHARE):
            if path.exists():
                self.assertTrue(path.is_dir() and not path.is_symlink(), path)
            else:
                path.mkdir()
                self.addCleanup(path.rmdir)
        work = GUEST_WORK / ("b9-work-" + self.nonce)
        self.assertFalse(work.exists() or work.is_symlink())
        self.addCleanup(lambda: shutil.rmtree(work) if work.exists() else None)
        sentinel, finished, ready, collector = [GUEST_SHARE / (stem + self.nonce + ext)
            for stem, ext in (("helper-executed-", ".txt"), ("finished-", ".json"), ("ready-", ".json"), ("collector-", ".json"))]
        for path in (sentinel, finished, ready, collector):
            self.assertFalse(path.exists() or path.is_symlink())
            self.addCleanup(path.unlink, missing_ok=True)
        helper_source = GUEST_SHARE / HELPER.name
        self.assertFalse(helper_source.exists() or helper_source.is_symlink())
        attack = f"[IO.File]::WriteAllText('{sentinel}', 'executed')\r\n".encode()
        with helper_source.open("xb") as output:
            self.addCleanup(helper_source.unlink)
            output.write(attack + HELPER.read_bytes())
        result = self.child("-ExpectedGuestScriptSha256", self.child_sha)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertTrue(finished.is_file(), result.stdout + result.stderr)
        observation = json.loads(finished.read_text(encoding="utf-8"))
        self.assertEqual(observation["failure_code"], "PREPARATION_FAILED")
        self.assertIn("B9 helper script differs from sealed source", observation["failure_detail"])
        self.assertEqual((observation["pid"], observation["collector_started"]), (0, False))
        self.assertFalse(any(path.exists() for path in (sentinel, ready, collector)))


if __name__ == "__main__":
    unittest.main(verbosity=2)
