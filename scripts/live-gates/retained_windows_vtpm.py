"""Export a lane's vTPM key and copy its state for a T19 handoff without echoing either child.

The packaged app's messages and cp's errors can name private paths. Both run
through retained_windows_quiet.run_quietly, which captures their output with a
bound and a timeout and refuses every failure with fixed text.
"""
from __future__ import annotations

from pathlib import Path
from retained_windows_quiet import run_quietly


def export_vtpm(executable: Path, slug: str, state: Path, package: Path, code: Path) -> None:
    arguments = ["--stable-vm-id", slug, "--state-dir", str(state), "--package", str(package),
                 "--recovery-code-file", str(code)]
    run_quietly("packaged vTPM export", [str(executable), "--vtpm-lifecycle", "export", *arguments])


def copy_vtpm_state(state: Path, destination: Path) -> None:
    """Clone the state tree into a new private directory."""
    destination.mkdir(mode=0o700)
    run_quietly("vTPM state copy", ["/bin/cp", "-cR", f"{state}/.", str(destination)])
