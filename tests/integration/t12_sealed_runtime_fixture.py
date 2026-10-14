"""Synthetic phase runner and signature verifier, explicitly never native/HVF."""
import json
import subprocess
import sys
from t12_sealed_test_support import Fixture, TIER, digest

RUNNER = r'''#!/usr/bin/env python3
import hashlib, json, pathlib, sys
trace = pathlib.Path(__TRACE__)
with trace.open("a") as stream: stream.write(json.dumps(sys.argv[1:]) + "\n")
path = pathlib.Path(sys.argv[sys.argv.index("--vars-file") + 1])
mode = sys.argv[sys.argv.index("--expect") + 1]
if mode == "written":
    if path.exists(): sys.exit(1)
    initial = bytes([255]) * 65536
    path.write_bytes(bytes([1]) * 65536)
else: initial = path.read_bytes()
final = hashlib.sha256(path.read_bytes()).hexdigest()
print("process_mode=" + mode)
print("dxe_result=" + ("10 variable_state=1" if mode == "written" else "11 variable_state=2"))
print("vars_loaded_sha256=" + hashlib.sha256(initial).hexdigest())
print("vars_file_sha256=" + final)
print("nvme_bar_reads=2 nvme_bar0=0x100000 nvme_bar0_len=16384 nvme_cap=0x20020103ff nvme_version=0x10400 nvme_command=0x7")
print("nvme_block_io=1 nvme_block_size=512 nvme_media_present=1 nvme_block_reads=1 nvme_last_block=2047 nvme_lba0_marker=0x4d56454744495242")
'''


class Runtime(Fixture):
    def __init__(self):
        super().__init__()
        self.trace = self.base / "synthetic-phases.jsonl"
        self.binary.write_text(RUNNER.replace("__TRACE__", repr(str(self.trace))))
        self.rows["binary"][1] = digest(self.binary); self.write_manifest()
        bins = self.base / "bin"; bins.mkdir()
        self.signature = bins / "codesign"
        self.signature.write_text('#!/bin/sh\n[ "$1" = --verify ] && [ "$2" = --strict ]\n')
        self.signature.chmod(0o700)
        if sys.platform != "darwin":
            # The production persistence script uses BSD stat; this fixture supplies only that query.
            query = bins / "stat"
            query.write_text('#!/usr/bin/env python3\nimport os, sys\nassert sys.argv[1:3] == ["-f", "%z"]\nprint(os.stat(sys.argv[3]).st_size)\n')
            query.chmod(0o700)
        self.env["PATH"] = str(bins) + ":" + self.env["PATH"]

    def queue_job(self, name="run"):
        result = self.submit(name)
        if result.returncode: raise AssertionError(result.stderr)
        job = self.queue / "running" / name
        (self.queue / "queued" / name).rename(job)
        return job

    def run_tier(self, job):
        return subprocess.run(["/bin/bash", str(self.scripts / "run-tier.sh"), TIER,
            "--out", str(job), "--job-id", job.name, "--input-manifest", str(job / "input-manifest.tsv"),
            "--sealed-binary", str(job / "hvf_gic_boot_probe")],
            env=self.env, capture_output=True, text=True, timeout=30)

    def phases(self):
        return [json.loads(row) for row in self.trace.read_text().splitlines()] if self.trace.exists() else []
