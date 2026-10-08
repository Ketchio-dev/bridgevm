"""Invoke the import receipt auditor with its prelaunch seal."""
import subprocess
import sys
import product_e2e_identity_fixtures as fixture

def command(self, include_hash: bool = True) -> subprocess.CompletedProcess:
    command = [sys.executable, str(fixture.ROOT / "scripts/live-gates/write-windows-import-product-e2e-receipt.py"),
               "--check-lane", str(self.result), "--request", str(self.request), "--stamp", str(self.stamp),
               "--job-id", fixture.JOB, "--commit", fixture.COMMIT, "--mode", "pilot", "--ordinal", "1"]
    if include_hash:
        command += ["--expected-request-sha256", self.prelaunch_hash]
    return subprocess.run(command, capture_output=True, text=True, timeout=30)
