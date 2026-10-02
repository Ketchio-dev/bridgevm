"""Publish without replacement and clean only this attempt's owned directory."""
from __future__ import annotations

import ctypes
import errno
import os
from pathlib import Path
import sys
import uuid
from retained_windows_cleanup import clear_directory
from retained_windows_identity import directory_identity


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


class OwnedRetention:
    def __init__(self, staging: Path):
        self.path = staging
        self.last_cleanup_error = None
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
        self.last_cleanup_error = None
        original = self.path
        step = "admit owned directory"
        try:
            if directory_identity(original) != self.identity:
                return False
            quarantine = original.with_name(f".{original.name}.cleanup-{uuid.uuid4().hex}")
            step = "quarantine owned directory"
            rename_exclusive(original, quarantine)
            self.path = quarantine
            if directory_identity(quarantine) != self.identity:
                # A name changed after admission. Restore it only when its old
                # name is still absent; otherwise preserve both entries.
                step = "restore replaced quarantine"
                rename_exclusive(quarantine, original)
                self.path = original
                return False
            step = "open owned directory"
            descriptor = os.open(self.path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            metadata = os.fstat(descriptor)
            if (metadata.st_dev, metadata.st_ino) != self.identity:
                return False
            step = "clear owned directory"
            clear_directory(descriptor)
            if directory_identity(self.path) != self.identity:
                return False
            step = "remove owned directory"
            os.rmdir(self.path)
            return True
        except OSError as error:
            self.last_cleanup_error = f"{step}: {type(error).__name__}: {error}"
            return False
        finally:
            if descriptor is not None:
                os.close(descriptor)
