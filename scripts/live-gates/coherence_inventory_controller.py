"""Observe current guest WINLIST without changing guest enumeration behavior."""
import hashlib
import json
import uuid

from guest_input_controller import Controller
from guest_input_protocol import regular_bytes
from coherence_inventory_observation import fixture, inventory, compare


class CoherenceController(Controller):
    def run(self):
        self.wait(lambda: any(line.startswith("BVAGENT SERVICE start") for line in self.lines()))
        if regular_bytes(self.control, 1):
            raise ValueError("exclusive empty control file required")
        name = "bv-coherence-multiwindow.ps1"
        guest = "C:\\BridgeVM\\input-proof\\" + name
        digest = hashlib.sha256(regular_bytes(self.share / name, 1024 * 1024)).hexdigest().upper()
        self.await_staged(guest, digest)
        script = ("$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments "
                  "@{CommandLine='powershell.exe -NoProfile -ExecutionPolicy Bypass -File " + guest
                  + " -Nonce " + self.nonce + "'}; if($r.ReturnValue -ne 0){exit 43}; "
                  "Write-Output 'BVCOHERENCE_STARTED " + self.nonce + "'")
        command = 'powershell.exe -NoProfile -Command "' + script + '"'
        self.send(command, command, "BVCOHERENCE_STARTED " + self.nonce)
        expected = fixture(self.file("coherence-ready-" + self.nonce + ".json"), self.nonce)
        command = "WINLIST " + str(uuid.uuid4())
        offset = self.write_command(command)
        response = self.wait(lambda: inventory(self.lines()[offset:], command))
        observation = compare(expected, response)
        with (self.share.parent / "coherence-observation.json").open("x") as output:
            json.dump(observation, output, sort_keys=True)
        with (self.share / ("coherence-stop-" + self.nonce)).open("x") as stop:
            stop.write("stop\n")
        return {"nonce": self.nonce, "guest_application_observed": False, "coherence": observation}
