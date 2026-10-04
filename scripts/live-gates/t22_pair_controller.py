"""Run one pinned, read-only query through the already running guest agent."""
import secrets

from guest_input_controller import Controller
from guest_input_protocol import regular_bytes
from t22_pair_admission import read_query

SCRIPT = "bv-t22-pair-admission.ps1"
GUEST_SHARE = r"C:\BridgeVM\t22-pair-proof"


class PairController(Controller):
    def __init__(self, control, log, share, script_hash, timeout=120):
        super().__init__(control, log, share, timeout)
        self.nonce = secrets.token_hex(16)
        self.script_hash = script_hash

    def run(self):
        self.wait(lambda: any(line.startswith("BVAGENT SERVICE start") for line in self.lines()))
        if regular_bytes(self.control, 1):
            raise ValueError("pair preparation requires an exclusive empty control file")
        self.await_staged(GUEST_SHARE + "\\" + SCRIPT, self.script_hash.upper())
        command = ("powershell.exe -NoProfile -ExecutionPolicy Bypass -File " + GUEST_SHARE
                   + "\\" + SCRIPT + " -Action Launch -Nonce " + self.nonce
                   + " -ExpectedSha256 " + self.script_hash)
        self.send(command, command, "T22-PAIR-QUERY-LAUNCHED-" + self.nonce)
        def query():
            try:
                return read_query(self.share, self.nonce, self.script_hash)
            except FileNotFoundError:
                return None
        return self.wait(query)
