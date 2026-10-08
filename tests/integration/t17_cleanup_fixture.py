"""Owned read-only payload for cleanup contracts."""

def make_readonly_payload(root):
    payload = root / "payload"
    payload.mkdir()
    for name in ("network", "storage", "serial"):
        directory = payload / name
        directory.mkdir()
        file = directory / "driver.inf"
        file.write_bytes(b"immutable driver fixture\n")
        file.chmod(0o400)
        directory.chmod(0o500)
    payload.chmod(0o500)
    return payload
