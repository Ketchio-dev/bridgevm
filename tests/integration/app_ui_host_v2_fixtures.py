"""Owned files only; fake Mach-O data is never launched or treated as a signature."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import app_ui_host_v2_manifest as manifest
import app_ui_host_v2_bundle as bundle
import app_ui_host_v2_cleanup as cleanup
import app_ui_host_v2_diagnostic as diagnostic
from app_ui_diagnostic import digest
from app_ui_host_manifest import platform


class Fixture:
    def __init__(self, case):
        temporary = tempfile.TemporaryDirectory(prefix="v2 owned contract ")
        case.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.commit = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
        self.sources = self.root / "sources"
        self.sources.mkdir()
        cpu = 0x01000007 if platform.machine() == "x86_64" else 0x0100000C
        header = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 0, 0, 0, 0)
        self.fields = {}
        for key, name in manifest.FILES.items():
            path = self.sources / name
            if key in ("binary", "launcher"):
                content = header + key.encode()
            elif key.endswith("_info"):
                metadata = bundle.bundle_metadata(bundle.ROLES["host"][2]) if key == "host_info" else bundle.driver_bundle_metadata()
                content = plistlib.dumps(metadata)
            else:
                content = b"fixture metadata, not a real code signature"
            path.write_bytes(content)
            self.fields[key] = (path, digest(path))
        self.input = self.root / "manifest.tsv"
        self.text = f"format\t{manifest.FORMAT}\ncommit\t{self.commit}\n" + "".join(
            f"{key}\t{path}\t{value}\n" for key, (path, value) in self.fields.items())
        self.input.write_text(self.text)
        self.output = self.root / "job"
        self.output.mkdir()
        shutil.copyfile(self.fields["binary"][0], self.output / manifest.FILES["binary"])
        manifest.seal_inputs(self.input, self.commit, self.output)
        shutil.copyfile(self.input, self.output / "input-manifest.tsv")
        (self.output / "job.env").write_text(f"job_id=fixture\ntier={manifest.TIER}\ncommit={self.commit}\n"
            f"input_manifest_sha256={digest(self.input)}\nsealed_binary_sha256={self.fields['binary'][1]}\n")
        self.private = self.output / "app-ui-private"
        self.private.mkdir(mode=0o700)
        for role, (name, executable, _, key) in bundle.ROLES.items():
            app = self.private / name
            (app / "Contents/MacOS").mkdir(parents=True)
            (app / "Contents/_CodeSignature").mkdir()
            for field, target in ((key, app / "Contents/MacOS" / executable),
                                  (role + "_info", app / "Contents/Info.plist"),
                                  (role + "_resources", app / "Contents/_CodeSignature/CodeResources")):
                shutil.copyfile(self.fields[field][0], target)
        self.roles = {}
        for pid, (role, (name, executable, identifier, key)) in enumerate(bundle.ROLES.items(), 4242):
            app = self.private / name
            self.roles[role] = dict(pid=pid, launch_date=1700000000.5, bundle_identifier=identifier,
                bundle_path=str(app), executable_path=str(app / "Contents/MacOS" / executable),
                executable_sha256=self.fields[key][1], identity_verified=True, exit_observed=True,
                cleanup_verified=True, termination="none", launch_requested=True, no_launch_verified=False, failure=None)
        self.launch = dict(schema_version=2, kind="native-app-ui-launcher-v2", started_uptime=100.0,
            deadline_uptime=190.0, **self.roles, cleanup_verified=True, success=True, failure=None,
            cancelled=False, timed_out=False)
        self.receipt = dict(tier=manifest.TIER, commit=self.commit, job_id="fixture",
            binary_hash=self.fields["binary"][1], helper_sha256=self.fields["launcher"][1],
            input_manifest_sha256=digest(self.input), run_count=1, claim_eligible=False,
            criterion_pass=False, capability_promotion=False)
        self.save()

    def save(self):
        (self.private / "launcher-observations-v2.json").write_text(json.dumps(self.launch))
        (self.output / "receipt.json").write_text(json.dumps(self.receipt))

    def write_driver(self):
        session = dict(schemaVersion=1, kind="native-app-ui-driver-session", nonce="a" * 64,
            startedUptime=100.0, deadlineUptime=190.0,
            **{role: diagnostic.protocol_identity(value) for role, value in self.roles.items()})
        path = self.private / "driver-session.json"
        path.write_text(json.dumps(session))
        binding = dict(schemaVersion=1, nonce=session["nonce"], sessionSHA256=digest(path))
        ready = dict(**binding, kind="native-app-ui-driver-ready", driver=session["driver"], trusted=True,
                     codeRequirementUnavailableReason="fixture-only: no signed executable was launched")
        completion = dict(**binding, kind="native-app-ui-driver-completion", hostPID=4242, driverPID=4243,
                          success=True, requestsProcessed=12, mutationsPerformed=6)
        (self.private / "driver-ready.json").write_text(json.dumps(ready))
        (self.private / "driver-completion.json").write_text(json.dumps(completion))
        return ready, completion
