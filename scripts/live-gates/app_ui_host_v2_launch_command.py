"""Keep the sealed supervisor inside its verified driver bundle signature context."""
from pathlib import Path

from app_ui_host_v2_bundle import ROLES
from app_ui_host_v2_manifest import verify_input


def supervisor_executable(private):
    name, executable, _, _ = ROLES["driver"]
    return Path(private) / name / "Contents/MacOS" / executable


def supervisor_command(launcher, private, host_digest, launcher_digest, mode):
    if Path(launcher) != supervisor_executable(private):
        raise ValueError("supervisor requires the reconstructed driver bundle executable")
    verify_input("launcher", Path(launcher), launcher_digest)
    return [str(launcher), mode, "--private-root", str(private),
            "--host-sha256", host_digest, "--launcher-sha256", launcher_digest]
