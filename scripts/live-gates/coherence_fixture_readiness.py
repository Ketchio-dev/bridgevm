"""Every candidate dependency must be hash-confirmed inside the guest."""
import hashlib
from guest_input_protocol import regular_bytes

FILES = ("bv-coherence-multiwindow.ps1", "bv-coherence-inventory-proof.ps1", "bv-window-inventory.cs")


def await_staged_fixture(controller):
    prefix = "C:\\BridgeVM\\input-proof\\"
    for name in FILES:
        data = regular_bytes(controller.share / name, 1024 * 1024)
        controller.await_staged(prefix + name, hashlib.sha256(data).hexdigest().upper())
    return prefix + FILES[0]
