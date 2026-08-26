"""Pick an openssl that can actually do Ed25519.

macOS ships /usr/bin/openssl as LibreSSL, which answers `genpkey -algorithm
ED25519` with "Algorithm ED25519 not found". Trusting bare `openssl` from PATH
therefore made this signer's result depend on the caller's environment: it
passed from an interactive shell that finds Homebrew's OpenSSL first, and
failed under scripts/check-project.sh, whose PATH resolves /usr/bin/openssl.

Probe for the capability instead of guessing from a version string, and refuse
with a named reason when no candidate has it. The signer must never silently
fall back to a weaker algorithm.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path

CANDIDATES = ("openssl", "/opt/homebrew/opt/openssl@3/bin/openssl",
              "/usr/local/opt/openssl@3/bin/openssl", "/opt/homebrew/bin/openssl")


def supports_ed25519(openssl: str) -> bool:
    binary = shutil.which(openssl) if "/" not in openssl else openssl
    if not binary or not Path(binary).exists():
        return False
    with tempfile.TemporaryDirectory() as temp:
        probe = Path(temp) / "probe.pem"
        completed = subprocess.run(
            [binary, "genpkey", "-algorithm", "ED25519", "-out", str(probe)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        # LibreSSL exits 0 while printing "Algorithm ED25519 not found" and
        # writing nothing, so the key file is the only trustworthy signal.
        return completed.returncode == 0 and probe.is_file() and probe.stat().st_size > 0


def resolve(requested: str, explicit: bool) -> str:
    """Return an Ed25519-capable openssl, or raise with a named reason."""
    if explicit:
        if supports_ed25519(requested):
            return requested
        raise RuntimeError(f"requested openssl {requested} cannot generate Ed25519 keys")
    for candidate in CANDIDATES:
        if supports_ed25519(candidate):
            return candidate
    raise RuntimeError("no available openssl supports Ed25519; install OpenSSL 1.1.1 or newer")
