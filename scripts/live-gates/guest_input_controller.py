#!/usr/bin/env python3
"""Bounded diagnostic for an already owned guest; not a product UI gate."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import time
import uuid
sys.path.insert(0, str(Path(__file__).resolve().parent))
from guest_input_protocol import (FIRST, SECOND, EXPECTED_HASH, regular_bytes,
                                  ready_coordinates, check_result, input_sequence)


def completed_reply(lines, label, expected):
    """Only a complete, successful native command envelope can advance input."""
    start = "BVAGENT CMD " + label + " exit="
    end = "BVAGENT END " + label
    starts = [i for i, line in enumerate(lines) if line.startswith(start)]
    if not starts:
        return False
    if len(starts) != 1 or lines[starts[0]] != start + "0":
        raise ValueError("duplicate or failed command reply")
    ends = [i for i, line in enumerate(lines) if line == end]
    if not ends:
        return False
    if len(ends) != 1 or ends[0] <= starts[0]:
        raise ValueError("invalid command completion envelope")
    body = [line for line in lines[starts[0] + 1:ends[0]] if line != ""]
    choices = (expected,) if isinstance(expected, str) else expected
    if len(body) != 1 or body[0] not in choices:
        raise ValueError("unexpected command receipt body")
    return body[0]


class Controller:
    def __init__(self, control, log, share, timeout=120):
        self.control, self.log, self.share = map(Path, (control, log, share))
        self.deadline = time.monotonic() + timeout
        self.nonce = str(uuid.uuid4())

    def wait(self, condition):
        while time.monotonic() < self.deadline:
            value = condition()
            if value:
                return value
            time.sleep(0.1)
        raise TimeoutError("guest input diagnostic deadline exceeded")

    def lines(self):
        try:
            data = regular_bytes(self.log, 64 * 1024 * 1024)
        except FileNotFoundError:
            return []
        return data.decode("utf-8", errors="replace").replace("\r", "\n").splitlines()

    def send(self, command, label, expected):
        offset = len(self.lines())
        fd = os.open(self.control, os.O_WRONLY | os.O_APPEND | os.O_NOFOLLOW)
        with os.fdopen(fd, "ab", buffering=0) as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                raise ValueError("control must be regular")
            stream.write((command + "\n").encode("ascii"))
        return self.wait(lambda: completed_reply(self.lines()[offset:], label, expected))

    def file(self, name):
        def read():
            try:
                return json.loads(regular_bytes(self.share / name, 8192))
            except FileNotFoundError:
                return None
        return self.wait(read)

    def input(self, verb, payload, count):
        identifier = str(uuid.uuid4())
        label = verb + " " + identifier
        encoded = base64.b64encode(payload.encode()).decode()
        self.send(label + " " + encoded, label, "BVINPUT_INSERTED " + identifier + " " + str(count))

    def dispatch_inputs(self, x, y):
        for verb, payload, count in input_sequence(x, y):
            self.input(verb, payload, count)

    def await_staged(self, guest, digest):
        # Only this read-only query may repeat. Input commands are never replayed.
        def probe():
            tag = str(uuid.uuid4())
            ready = "BVINPUT_STAGED " + tag
            pending = "BVINPUT_PENDING " + tag
            script = ("$p='" + guest + "'; if((Test-Path -LiteralPath $p) -and "
                      "((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash -ceq '"
                      + digest + "')){Write-Output '" + ready + "'}else{Write-Output '" + pending + "'}")
            command = 'powershell.exe -NoProfile -Command "' + script + '"'
            response = self.send(command, command, (ready, pending))
            return response == ready
        self.wait(probe)

    def run(self):
        self.wait(lambda: any(line.startswith("BVAGENT SERVICE start") for line in self.lines()))
        if regular_bytes(self.control, 1):
            raise ValueError("diagnostic requires exclusive empty control file")
        cap = "INPUTCAPS " + str(uuid.uuid4())
        self.send(cap, cap, "BVINPUT_CAPS " + cap.split()[1] + " 3 TEXTINPUT KEYINPUT POINTERINPUT 65536")
        sink = self.share / "bv-input-order-sink.ps1"
        digest = hashlib.sha256(regular_bytes(sink, 1024 * 1024)).hexdigest().upper()
        guest = r"C:\BridgeVM\input-proof\bv-input-order-sink.ps1"
        self.await_staged(guest, digest)
        # Invoke-CimMethod starts an independent process; the resident pipe stays free.
        script = ("$p='" + guest + "'; if((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash -cne '"
                  + digest + "'){exit 42}; $r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create "
                  "-Arguments @{CommandLine='powershell.exe -NoProfile -ExecutionPolicy Bypass -File "
                  + guest + " -Nonce " + self.nonce + "'}; if($r.ReturnValue -ne 0){exit 43}; "
                  "Write-Output 'BVINPUT_SINK_STARTED " + self.nonce + "'")
        command = 'powershell.exe -NoProfile -Command "' + script + '"'
        self.send(command, command, "BVINPUT_SINK_STARTED " + self.nonce)
        x, y = ready_coordinates(self.file("ready-" + self.nonce + ".json"), self.nonce)
        self.dispatch_inputs(x, y)
        check_result(self.file("result-" + self.nonce + ".json"), self.nonce)
        return {"schema": "bridgevm.guest-input-diagnostic.v1", "nonce": self.nonce,
                "guest_application_observed": True, "production_ui_proven": False,
                "claim_eligible": False, "criterion_pass": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("control", "log", "share"):
        parser.add_argument("--" + name, required=True)
    args = parser.parse_args()
    try:
        result = Controller(args.control, args.log, args.share).run()
    except (OSError, ValueError, TimeoutError) as error:
        print(json.dumps({"claim_eligible": False, "guest_application_observed": False,
                          "failure_type": type(error).__name__}))
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
