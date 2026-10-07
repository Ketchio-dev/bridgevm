"""Identity-bound product E2E work below the run's private result directory."""
from pathlib import Path
import os
import re
import stat

FIELDS = ("work_parent", "work_parent_identity", "work_identity")
JOB = r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}"


def directory(path):
    path = Path(path)
    if not path.is_absolute() or str(path) != os.path.normpath(str(path)) or path.resolve() != path:
        raise ValueError("work directory is not canonical")
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or stat.S_IMODE(info.st_mode) != 0o700:
        raise ValueError("work directory is not private and owned")
    return f"{info.st_dev}:{info.st_ino}"


def layout(root, parent, job, kind, lane=None):
    raw_root, raw_parent = str(root), str(parent)
    root, parent = Path(root), Path(parent)
    if str(root) != raw_root or str(parent) != raw_parent or raw_root != os.path.normpath(raw_root):
        raise ValueError("noncanonical work spelling")
    if kind not in ("e2e", "import-e2e") or not re.fullmatch(JOB, job):
        raise ValueError("invalid work identity")
    if not parent.is_absolute() or str(parent) != os.path.normpath(str(parent)):
        raise ValueError("invalid work parent")
    work = root.parent if lane is not None else root
    if lane is not None and (type(lane) is not int or lane not in (1, 2, 3) or root.name != f"lane-{lane}"):
        raise ValueError("lane differs from its ordinal")
    if work.parent != parent or not re.fullmatch(r"bridgevm-" + kind + "-" + re.escape(job) + r"\.[A-Za-z0-9]{6}", work.name):
        raise ValueError("work is outside its exact parent/job boundary")
    return work


def cleanup_parent(root, parent, job, expected):
    kind = "import-e2e" if Path(root).name.startswith("bridgevm-import-e2e-") else "e2e"
    layout(root, parent, job, kind)
    if directory(parent) != expected:
        raise ValueError("cleanup parent identity changed")
    fd = os.open(parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    info = os.fstat(fd)
    if f"{info.st_dev}:{info.st_ino}" != expected:
        os.close(fd)
        raise ValueError("cleanup parent changed during open")
    return fd


def capture(root, job, kind, lane):
    root = Path(root)
    work = layout(root, root.parent.parent, job, kind, lane)
    parent = work.parent
    result = {"work_parent": str(parent), "work_parent_identity": directory(parent),
              "work_identity": directory(work)}
    directory(root)
    if parent.stat().st_dev != work.stat().st_dev or work.stat().st_dev != root.stat().st_dev:
        raise ValueError("work crossed a device")
    return result


def validate(request, kind):
    for field in FIELDS:
        if not isinstance(request.get(field), str):
            raise ValueError("missing work binding")
    work = layout(request["lane_root"], request["work_parent"], request["job_id"], kind, request["lane"])
    if capture(request["lane_root"], request["job_id"], kind, request["lane"]) != {key: request[key] for key in FIELDS}:
        raise ValueError("work binding changed")
    return work
