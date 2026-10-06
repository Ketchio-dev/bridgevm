"""Clear retained trees through checked directory descriptors, never symlinks."""
from __future__ import annotations

import errno
import os
import stat


def clear_directory(descriptor: int) -> None:
    # Directory write permission permits unlinking even read-only files. Work
    # through held descriptors so symlink entries never redirect chmod or removal.
    os.fchmod(descriptor, 0o700)
    for name in os.listdir(descriptor):
        metadata = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
        if stat.S_ISDIR(metadata.st_mode):
            child = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                            dir_fd=descriptor)
            try:
                opened = os.fstat(child)
                if (opened.st_dev, opened.st_ino) != (metadata.st_dev, metadata.st_ino):
                    raise OSError(errno.ESTALE, "retention child was replaced before cleanup")
                clear_directory(child)
            finally:
                os.close(child)
            remaining = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
            if (remaining.st_dev, remaining.st_ino) != (opened.st_dev, opened.st_ino):
                raise OSError(errno.ESTALE, "retention child was replaced during cleanup")
            os.rmdir(name, dir_fd=descriptor)
        else:
            os.unlink(name, dir_fd=descriptor)
