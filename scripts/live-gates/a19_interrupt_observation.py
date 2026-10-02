"""Private exclusive observation files and the exact read-only FD parser."""
import os
from pathlib import Path
import re

FD = re.compile(r"f[0-9]+\Z")


def private_new(path: Path, mode: str):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    return os.fdopen(descriptor, mode)


def read_fd_observed(data: str, pid: int, staged_disk: Path) -> bool:
    owner = False
    current_fd = ""
    access = ""
    for line in data.splitlines():
        if line.startswith("p"):
            owner = line == f"p{pid}"
            current_fd = ""
            access = ""
        elif line.startswith("f"):
            current_fd = line
            access = ""
        elif line.startswith("a"):
            access = line
        elif line.startswith("n") and owner and FD.fullmatch(current_fd) and access == "ar":
            if line[1:] == str(staged_disk):
                return True
    return False
