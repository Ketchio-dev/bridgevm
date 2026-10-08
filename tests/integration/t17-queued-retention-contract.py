#!/usr/bin/env python3
"""Exercise normal dispatcher retention and verify unchanged paths after job move."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root, temporary, manifest = map(Path, sys.argv[1:])
queue = temporary.resolve() / "retention-queue"
queue.mkdir(mode=0o700)
for state in ("running", "done"):
    (queue / state).mkdir(mode=0o700)
job = "queued-handoff-fixture"
output = queue / "running" / job
output.mkdir(mode=0o700)
subprocess.run([str(root / "scripts/live-gates/run-special-tier.sh"),
                "t17-windows-hvf-product-e2e", str(output), job, str(manifest)], check=True)
status = output / "private/t19-source-handoff.json"
raw = status.read_bytes()
value = json.loads(raw)
destination = queue / "t19-sources" / job
assert value["destination"] == str(destination) and value["verified"] is True
retained = destination / "t19-input-manifest.tsv"
assert hashlib.sha256(retained.read_bytes()).hexdigest() == value["manifest_sha256"]
finished = queue / "done" / job
output.rename(finished)
assert (finished / "private/t19-source-handoff.json").read_bytes() == raw
assert hashlib.sha256(retained.read_bytes()).hexdigest() == value["manifest_sha256"]
subprocess.run([sys.executable, str(root / "scripts/live-gates/windows-import-product-e2e-manifest.py"),
                "--manifest", str(retained), "--out", str(temporary / "queued-retention-verified.json")], check=True)
print("PASS: queued T17 retains immutable same-volume T19 inputs across running-to-done")
