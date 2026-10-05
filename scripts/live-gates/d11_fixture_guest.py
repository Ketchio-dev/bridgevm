"""A new nonce-bound development account only, through the running service."""
import hashlib
import json
import time

from guest_input_controller import Controller
from guest_input_protocol import regular_bytes
from d11_fixture_files import unique

SCRIPT = "bv-d11-fixture-ready.ps1"
SHARE = r"C:\BridgeVM\d11-fixture"


def facts(raw, nonce, script_hash):
    value = json.loads(raw, object_pairs_hook=unique)
    fixed = {"schema": "bridgevm.d11-fixture-ready.v1", "nonce": nonce,
             "script_sha256": script_hash, "account_bound": True,
             "password_never_expires": True, "agent_task_registered": True, "desktop_observed": True}
    if (type(value) is not dict or set(value) != set(fixed) | {"autologon_remaining"}
            or any(type(value[k]) is not type(v) or value[k] != v for k, v in fixed.items())
            or type(value["autologon_remaining"]) is not int or not 1 <= value["autologon_remaining"] <= 4):
        raise ValueError("fresh fixture readiness facts refused")
    return value


class FixtureController(Controller):
    def __init__(self, control, log, share, nonce, script_hash, processes):
        super().__init__(control, log, share, timeout=360)
        self.nonce, self.script_hash, self.processes = nonce, script_hash, processes

    def wait(self, condition):
        while time.monotonic() < self.deadline:
            self.processes.checkpoint()
            if self.processes.process.poll() is not None: raise ValueError("fixture guest exited before reply")
            value = condition()
            if value: return value
            time.sleep(.1)
        raise TimeoutError("fixture service phase deadline")

    def ready(self):
        lines = self.lines()
        return (any(line.startswith("BVAGENT READY") for line in lines)
                and any(line.startswith("BVAGENT SERVICE start") for line in lines))

    def write_command(self, command):
        if not self.ready(): raise ValueError("fixture control requires READY and SERVICE")
        return super().write_command(command)

    def run(self):
        self.wait(self.ready)
        if regular_bytes(self.control, 1): raise ValueError("fixture control is not exclusive")
        guest = SHARE + "\\" + SCRIPT
        self.await_staged(guest, self.script_hash.upper())
        command = ("powershell.exe -NoProfile -ExecutionPolicy Bypass -File " + guest
                   + " -Action Launch -Nonce " + self.nonce + " -ExpectedSha256 " + self.script_hash)
        self.send(command, command, "D11-FIXTURE-LAUNCHED-" + self.nonce)
        def result():
            try:
                raw = regular_bytes(self.share / ("result-" + self.nonce + ".json"), 8192)
                done = regular_bytes(self.share / ("result-" + self.nonce + ".done"), 64)
            except FileNotFoundError:
                return None
            if done != hashlib.sha256(raw).hexdigest().encode(): raise ValueError("fixture reply seal differs")
            facts(raw, self.nonce, self.script_hash)
            return hashlib.sha256(raw).hexdigest()
        return self.wait(result)
