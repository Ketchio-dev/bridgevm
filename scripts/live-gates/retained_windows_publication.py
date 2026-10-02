"""Publish without replacement and clean only this attempt's owned directory."""
from __future__ import annotations

import ctypes
import errno
import os
from pathlib import Path
import stat
import sys
import uuid
from retained_windows_cleanup import clear_directory


def rename_exclusive(source: Path, destination: Path) -> None:
    native = ctypes.CDLL(None, use_errno=True)
    try:
        if sys.platform == "darwin":
            move = native.renamex_np
            move.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
            arguments = (os.fsencode(source), os.fsencode(destination), 0x4)  # RENAME_EXCL
        elif sys.platform.startswith("linux"):
            move = native.renameat2
            move.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int,
                             ctypes.c_char_p, ctypes.c_uint]
            arguments = (-100, os.fsencode(source), -100, os.fsencode(destination), 1)
        else:
            raise AttributeError("exclusive rename is unavailable")
    except AttributeError as error:
        raise OSError(errno.ENOTSUP, "exclusive retention publication is unavailable") from error
    move.restype = ctypes.c_int
    if move(*arguments) != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), str(destination))


def directory_identity(path: Path) -> tuple[int, int] | None:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return None
    if not stat.S_ISDIR(metadata.st_mode):
        return None
    return metadata.st_dev, metadata.st_ino


class OwnedRetention:
    def __init__(self, staging: Path):
        self.path = staging
        self.identity = directory_identity(staging)
        if self.identity is None:
            raise ValueError("retention staging is not an owned directory")

    def publish(self, destination: Path) -> None:
        if directory_identity(self.path) != self.identity:
            raise ValueError("retention staging was replaced before publication")
        rename_exclusive(self.path, destination)
        self.path = destination
        if directory_identity(self.path) != self.identity:
            raise ValueError("retention directory changed during publication")

    def cleanup(self) -> bool:
        descriptor = None
        original = self.path
        try:
            if directory_identity(original) != self.identity:
                return False
            quarantine = original.with_name(f".{original.name}.cleanup-{uuid.uuid4().hex}")
            rename_exclusive(original, quarantine)
            self.path = quarantine
            if directory_identity(quarantine) != self.identity:
                # A name changed after admission. Restore it only when its old
                # name is still absent; otherwise preserve both entries.
                rename_exclusive(quarantine, original)
                self.path = original
                return False
            descriptor = os.open(self.path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            metadata = os.fstat(descriptor)
            if (metadata.st_dev, metadata.st_ino) != self.identity:
                return False
            clear_directory(descriptor)
            if directory_identity(self.path) != self.identity:
                return False
            os.rmdir(self.path)
            return True
        except OSError:
            return False
        finally:
            if descriptor is not None:
                os.close(descriptor)
