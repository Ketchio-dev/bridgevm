"""Export the selected stopped pair under its product lease for a T19 handoff."""
from __future__ import annotations

from pathlib import Path
from windows_selected_media_cli import run, selected


def export_selected(request: dict, authenticated: dict, staging: Path) -> tuple[Path, Path]:
    pair = selected(request)
    if pair["disk_sha256"] != authenticated["final_disk_sha256"]:
        raise ValueError("selected disk differs from authenticated T17 output")
    if pair["vars_sha256"] != authenticated["final_vars_sha256"] or pair["vars_bytes"] != 64 * 1024 * 1024:
        raise ValueError("selected UEFI variables differ from authenticated T17 output")
    quota = pair["disk_bytes"] + pair["vars_bytes"]
    if quota > 2**64 - 1:
        raise ValueError("selected media pair exceeds the snapshot quota range")
    export = staging / "selected-media"
    run(request, "create", request["disk_path"], request["vars_path"], str(export), request["vm_slug"], str(quota))
    disk, variables = export / "disk.raw", export / "vars.fd"
    for path, field in ((disk, "disk_bytes"), (variables, "vars_bytes")):
        if not path.is_file() or path.is_symlink() or path.stat().st_size != pair[field]:
            raise ValueError("selected media export is missing or unsafe")
    return disk, variables
