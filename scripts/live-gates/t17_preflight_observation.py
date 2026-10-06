"""Bounded T17 observation bytes and an immutable manifest selection."""
import contextlib
import json
import os
from pathlib import Path
import stat
import tempfile

LIMIT = 32 * 1024


class PreflightError(ValueError):
    pass


def read_bounded(path: Path, *, owned: bool, label: str) -> bytes:
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            with os.fdopen(descriptor, "rb", closefd=False) as source:
                before = os.fstat(source.fileno())
                if not stat.S_ISREG(before.st_mode) or (owned and before.st_uid != os.geteuid()):
                    raise PreflightError(f"{label} is not an owned regular file" if owned else f"{label} is not regular")
                if not 0 < before.st_size <= LIMIT:
                    raise PreflightError(f"{label} is outside the 32 KiB bound")
                data = source.read(LIMIT + 1)
                after = os.fstat(source.fileno())
        finally:
            os.close(descriptor)
    except OSError as error:
        raise PreflightError(f"{label} cannot be safely read") from error
    fields = ("st_dev", "st_ino", "st_mode", "st_uid", "st_size", "st_mtime_ns", "st_ctime_ns")
    if any(getattr(before, key) != getattr(after, key) for key in fields):
        raise PreflightError(f"{label} changed while reading")
    if not 0 < len(data) <= LIMIT or len(data) != after.st_size:
        raise PreflightError(f"{label} is outside the 32 KiB bound")
    return data


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise PreflightError("diagnostic output contains duplicate JSON fields")
        result[key] = value
    return result


def nonfinite(_value):
    raise PreflightError("diagnostic output contains a nonfinite JSON value")


def read_report(path: Path) -> object:
    data = read_bounded(path, owned=True, label="diagnostic output")
    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=unique_fields, parse_constant=nonfinite)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise PreflightError("diagnostic output is not valid UTF-8 JSON") from error


@contextlib.contextmanager
def captured_manifest(path: Path):
    try:
        data = read_bounded(path, owned=False, label="T17 manifest")
    except PreflightError as error:
        raise PreflightError(f"T17 manifest is invalid: {error}") from error
    with tempfile.TemporaryDirectory(prefix="bridgevm-t17-manifest.") as directory:
        snapshot = Path(directory) / "manifest.tsv"
        with snapshot.open("xb") as output:
            output.write(data)
        snapshot.chmod(0o400)
        def unchanged():
            try:
                current = read_bounded(path, owned=False, label="T17 manifest")
            except PreflightError as error:
                raise PreflightError("T17 manifest changed during LaunchServices observation") from error
            if current != data:
                raise PreflightError("T17 manifest changed during LaunchServices observation")
        yield snapshot, unchanged
