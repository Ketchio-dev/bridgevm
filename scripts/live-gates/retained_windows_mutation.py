"""Mutate retained directory names and permissions without following aliases."""
from __future__ import annotations

import ctypes
import errno
import os
from pathlib import Path
import sys


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


def open_owned_directory_for_cleanup(path: Path, identity: tuple[int, int]) -> int:
    # macOS may require directory write permission before renaming a locked tree.
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        metadata = os.fstat(descriptor)
        if (metadata.st_dev, metadata.st_ino) != identity:
            raise OSError(errno.ESTALE, "retention directory was replaced before unlocking", str(path))
        os.fchmod(descriptor, 0o700)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise
