"""Identity checks for the staged resident input assets."""
import hashlib
from pathlib import Path


def verify_input_assets(staged: Path, receipt: str) -> None:
    for name in ("bvagent.ps1", "bvagent-firstboot.ps1", "bvagent-input.ps1", "bvagent-unicode-input.cs", "bvagent-task.ps1"):
        path = staged / "agent" / name
        assert path.is_file(), f"missing staged guest asset: {name}"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        expected = f"guest_tool\tagent/{name}\t{digest}"
        assert receipt.splitlines().count(expected) == 1, f"missing/duplicate staged identity: {name}"
