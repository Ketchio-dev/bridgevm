"""Synthetic DTO/image bytes only; no application, AX or signature is exercised."""
import json
from pathlib import Path
import struct
import zlib

from app_ui_host_v2_fixtures import Fixture as BundleFixture, ROOT, digest
import app_ui_install_cancel_manifest as manifest
import app_ui_install_cancel_cleanup as cleanup
import app_ui_install_cancel_protocol as protocol
import app_ui_install_cancel_observations as observations
from app_ui_host_v2_diagnostic import protocol_identity
from app_ui_install_cancel_transcript import PHASES, LABELS, FAILURE


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True))


def png():
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)) + \
        chunk(b"IDAT", zlib.compress(b"\0\0\0\0")) + chunk(b"IEND", b"")


class Fixture(BundleFixture):
    def __init__(self, case):
        super().__init__(case)
        self.text = self.text.replace("bridgevm-app-ui-host-v2", manifest.FORMAT)
        self.input.write_text(self.text)
        (self.output / "input-manifest.tsv").write_text(self.text)
        (self.output / "job.env").write_text(f"job_id=fixture\ntier={manifest.TIER}\ncommit={self.commit}\n"
            f"input_manifest_sha256={digest(self.input)}\nsealed_binary_sha256={self.fields['binary'][1]}\n")
        self.launch["kind"] = manifest.LAUNCHER_KIND
        self.receipt.update(tier=manifest.TIER, input_manifest_sha256=digest(self.input))
        save(self.private / manifest.LAUNCHER_FILE, self.launch)
        save(self.output / "receipt.json", self.receipt)
        self.session = dict(schemaVersion=1, kind="native-app-ui-install-cancel-session", nonce="a" * 64,
            startedUptime=100.0, deadlineUptime=190.0,
            **{role: protocol_identity(value) for role, value in self.roles.items()})
        save(self.private / "install-cancel-session.json", self.session)
        self.binding = dict(schemaVersion=1, nonce=self.session["nonce"],
            sessionSHA256=digest(self.private / "install-cancel-session.json"))
        self.ready = dict(**self.binding, kind="native-app-ui-install-cancel-ready",
            driver=self.session["driver"], trusted=True, codeRequirementUnavailableReason="synthetic fixture; no AX")
        self.completion = dict(**self.binding, kind="native-app-ui-install-cancel-completion", hostPID=4242,
            driverPID=4243, success=True, requestsProcessed=8, mutationsPerformed=3)
        save(self.private / "install-cancel-ready.json", self.ready)
        save(self.private / "install-cancel-completion.json", self.completion)
        for name in ("driver-requests", "driver-replies", "host-observations"):
            (self.private / name).mkdir()
        self.reads = {}
        actions = ((0, "readState"), (1, "start"), (1, "readState"), (2, "cancel"),
                   (2, "readState"), (3, "readState"), (4, "retry"), (4, "readState"))
        for sequence, (index, operation) in enumerate(actions, 1):
            request = dict(**self.binding, kind="native-app-ui-install-cancel-request", sequence=sequence,
                phase=PHASES[index], phaseDeadlineUptime=105.0 + index * 5, window="main", operation=operation)
            path = self.private / "driver-requests" / f"{sequence:06d}.json"
            save(path, request)
            reply = dict(**self.binding, kind="native-app-ui-install-cancel-reply", sequence=sequence,
                requestSHA256=digest(path), hostPID=4242, driverPID=4243, operation=operation,
                outcome="observed" if operation == "readState" else "performed")
            if operation == "readState":
                reply["values"] = dict(installVisible=True, startVisible=True, startEnabled=index in (0, 3, 4),
                    cancelVisible=index in (1, 2), cancelEnabled=index == 1, stage=LABELS[index],
                    title="설치 시작" if index < 3 else "다시 시도")
                if index == 4:
                    reply["values"]["failure"] = FAILURE
                self.reads[sequence] = PHASES[index]
            save(self.private / "driver-replies" / path.name, reply)
        self.observed = self.private / "host-observations"
        self.report = dict(schema_version=1, kind="native-app-ui-install-cancel-observations", fixture_data=True,
            guest_behavior_proof=False, screenshots=[], checkpoints=[], counters=dict(observations.COUNTERS),
            fixture_files=[], failure=None)
        for name in observations.FIXTURE_FILES:
            path = self.observed / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"synthetic verifier-only metadata " + name.encode())
            self.report["fixture_files"].append(dict(path=name, bytes=path.stat().st_size, sha256=digest(path)))
        for index, name in enumerate(observations.CAPTURES):
            path = self.observed / (name + ".png")
            path.write_bytes(png())
            self.report["screenshots"].append(dict(name=name, file=path.name, sha256=digest(path), width=1, height=1))
            self.report["checkpoints"].append(dict(phase=PHASES[index],
                stage=("idle", "validating", "cancelling", "cancelled", "failed")[index],
                is_running=index in (1, 2), can_cancel=index == 1, validator_calls=(0, 1, 1, 1, 2)[index],
                validator_finished=(0, 0, 0, 1, 2)[index], process_resets=(0, 1, 1, 1, 2)[index],
                process_cancels=int(index >= 2), fixture_preserved=True, read_sequence=(1, 3, 5, 6, 8)[index]))
        self.write_report()
        save(self.observed / "host-identity.json", dict(schema_version=1, kind="native-app-ui-host-identity",
            **{key: self.roles["host"][key] for key in ("pid", "bundle_identifier", "bundle_path",
                "executable_path", "executable_sha256")}, started_uptime=100.1))
        self.host_completion = dict(schemaVersion=1, kind="native-app-ui-install-cancel-host-completion",
            pid=4242, success=True, settled=True, fixturePreserved=True,
            reportSHA256=digest(self.observed / "install-cancel-observations.json"))
        save(self.observed / "install-cancel-host-completion.json", self.host_completion)

    def write_report(self):
        save(self.observed / "install-cancel-observations.json", self.report)
