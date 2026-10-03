"""Request supported host diagnostic shutdown before owned group cleanup."""
import os
from pathlib import Path
import subprocess


def diagnostic_stop(request: Path, process: subprocess.Popen) -> None:
    if process.poll() is not None or request.exists():
        return
    fd = os.open(request, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(b"")  # The host parser's supported legacy D3 request is empty.
    try:
        process.wait(timeout=35)
    except subprocess.TimeoutExpired:
        pass
