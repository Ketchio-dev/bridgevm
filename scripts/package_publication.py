"""Publish a verified sibling archive atomically without replacing any entry."""
import ctypes
import errno
import os
import sys


def publish_exclusive(source, destination_name, destination_fd):
    native = ctypes.CDLL(None, use_errno=True)
    try:
        if sys.platform == "darwin":
            move, flags = native.renameatx_np, 0x4  # RENAME_EXCL
        elif sys.platform.startswith("linux"):
            move, flags = native.renameat2, 1  # RENAME_NOREPLACE
        else:
            raise AttributeError
    except AttributeError as error:
        raise OSError(errno.ENOTSUP, "exclusive archive publication is unavailable") from error
    move.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int,
                     ctypes.c_char_p, ctypes.c_uint]
    move.restype = ctypes.c_int
    source_fd = os.open(source.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        # Both names are single leaves bound to already-open parent directories.
        # Unsupported filesystems fail closed; never fall back to ordinary rename.
        if move(source_fd, os.fsencode(source.name), destination_fd,
                os.fsencode(destination_name), flags) != 0:
            code = ctypes.get_errno()
            raise OSError(code, os.strerror(code), destination_name)
    finally:
        os.close(source_fd)
