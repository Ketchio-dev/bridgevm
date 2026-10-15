"""Explicit certificate selection and bounded codesign for diagnostic inputs."""
import re
import subprocess


def normalize_identity(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9A-Fa-f]{40}", value):
        raise ValueError("signing identity must be an exact 40-hex certificate fingerprint")
    return value.upper()


def code(command):
    result = subprocess.run(["/usr/bin/codesign", *command], capture_output=True, text=True, timeout=20)
    if result.returncode:
        raise ValueError("codesign refused: " + result.stderr.strip())
    return {"arguments": command, "exit_code": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def sign(bundle, identifier, identity=None):
    selected = "-" if identity is None else normalize_identity(identity)
    signed = code(["--force", "--sign", selected, "--identifier", identifier,
                   "--timestamp=none", str(bundle)])
    for path in bundle.rglob("*"):
        if path.is_dir():
            path.chmod(0o700)
        elif path.is_file():
            path.chmod(0o500 if path.parent.name == "MacOS" else 0o400)
    requirement = [] if identity is None else ["-R", f'=certificate leaf = H"{selected}"']
    verified = code(["--verify", "--strict", "--verbose=4", *requirement, str(bundle)])
    display = code(["--display", "--verbose=4", "--requirements", "-", str(bundle)])
    return {"sign": signed, "verify": verified, "display": display}
