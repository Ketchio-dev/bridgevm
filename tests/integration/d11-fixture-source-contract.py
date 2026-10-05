#!/usr/bin/env python3
"""Queue/worker wiring and fixed credential/provisioning source boundaries."""
import unittest

from d11_fixture_test_support import ROOT


class Source(unittest.TestCase):
    def text(self, path): return (ROOT / path).read_text()

    def test_tier_reaches_all_required_dispatch_boundaries(self):
        for name in ("bridgevm-live", "run-tier.sh", "run-special-tier.sh", "run-development-tier.sh",
                     "app-ui-manifest-dispatch.sh", "development-queue-receipt-dispatch.sh",
                     "development-worker-cleanup-dispatch.sh", "t17-worker-cleanup-fence.sh",
                     "bridgevm_live_receipt_route.py", "bridgevm_live_receipt_legacy.py"):
            self.assertIn("d11-native-fixture-preparation", self.text("scripts/live-gates/" + name), name)

    def test_source_admission_precedes_repository_import(self):
        source = self.text("scripts/live-gates/d11-fixture-queue-dispatch.sh")
        self.assertLess(source.index("t22-pair-source-admission.sh"), source.index("d11_fixture_queue.py"))
        self.assertIn("--noprofile --norc -p", source)
        self.assertIn("/usr/bin/env -i", source)
        self.assertIn("-B -s -E", source)

    def test_guest_asset_uses_file_cim_and_never_reads_password(self):
        raw = (ROOT / "scripts/win-assets/bv-d11-fixture-ready.ps1").read_bytes()
        self.assertNotIn(b"\n", raw.replace(b"\r\n", b""))
        source = raw.decode()
        self.assertIn("Invoke-CimMethod -ClassName Win32_Process -MethodName Create", source)
        self.assertIn("-File C:\\BridgeVM\\d11-fixture\\bv-d11-fixture-ready.ps1", source)
        self.assertIn("Get-LocalUser -Name $username", source)
        self.assertNotIn("-Name DefaultPassword", source)
        self.assertNotIn("Get-ItemProperty -", source)
        self.assertIn("[IO.FileMode]::CreateNew", source)

    def test_product_template_has_no_d11_or_auto_credentials(self):
        source = self.text("scripts/win-assets/unattend.xml")
        self.assertNotIn("AutoLogon", source)
        self.assertNotIn("BVT17", source)
        self.assertNotIn("d11-fixture", source)

    def test_helper_reuses_existing_algorithms(self):
        source = self.text("scripts/development/D11FixtureHelper.swift")
        self.assertIn("T17PrivateUnattend.write", source)
        self.assertIn("HvfWindowsBootSeed.seedFile", source)
        self.assertNotIn("SecRandomCopyBytes", source)
        self.assertNotIn("Process()", source)


if __name__ == "__main__": unittest.main()
