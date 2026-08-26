"""Round-trip self-test for the Ed25519 kernel-policy signer."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

HASH = re.compile(r"^[0-9a-f]{64}$")


def self_test(openssl: str, sign_package, Refusal, REPORT: str) -> None:
    with tempfile.TemporaryDirectory() as temp_text:
        temp = Path(temp_text)
        key = temp / "key.pem"
        subprocess.run(
            [openssl, "genpkey", "-algorithm", "ED25519", "-out", str(key)],
            check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        os.chmod(key, 0o600)
        source = temp / "source"
        source.mkdir()
        payloads = {
            "viogpu3d.inf": b"signed inf\n",
            "viogpu3d.sys": b"signed sys\n",
            "viogpu3d.cat": b"signed cat\n",
        }
        for name, payload in payloads.items():
            (source / name).write_bytes(payload)
        lines = [
            "BridgeVM viogpu3d Windows WDK finalization",
            "finalization_complete=true",
            "signing_mode=kernel-policy",
            "test_signing_required=false",
            "sys_kernel_policy_verified=true",
            "cat_kernel_policy_verified=true",
        ]
        lines.extend(f"sha256.{name}={hashlib.sha256(payload).hexdigest()}" for name, payload in payloads.items())
        (source / REPORT).write_text("\n".join(lines) + "\n", encoding="ascii")
        output = temp / "signed"
        result = sign_package(
            source, output, key, "self-test-key", "self-test-package",
            "2026-08-25T00:00:00Z", "2026-08-26T00:00:00Z", openssl,
        )
        if not HASH.fullmatch(result["attestation_sha256"]):
            raise AssertionError("self-test did not return an attestation hash")
        mutated = temp / "mutated"
        shutil.copytree(source, mutated)
        with (mutated / REPORT).open("a", encoding="ascii") as stream:
            stream.write("signing_mode=test\n")
        try:
            sign_package(
                mutated, temp / "refused", key, "self-test-key", "mutated",
                "2026-08-25T00:00:00Z", "2026-08-26T00:00:00Z", openssl,
            )
        except Refusal:
            pass
        else:
            raise AssertionError("duplicate report policy was accepted")
    print("PASS: signed kernel-policy package attestation self-test")

