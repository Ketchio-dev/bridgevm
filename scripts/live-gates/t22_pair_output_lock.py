"""Seal only admitted, unaliased output inodes through held descriptors."""
from contextlib import contextmanager
import os
import stat

from retained_windows_identity import directory_identity


@contextmanager
def seal_tree(output, identity):
    descriptors, entries, seen = [], [], set()
    def visit(fd):
        for name in os.listdir(fd):
            info = os.stat(name, dir_fd=fd, follow_symlinks=False)
            directory = stat.S_ISDIR(info.st_mode)
            if (len(entries) >= 512 or info.st_uid != os.geteuid()
                    or info.st_dev != identity[0] or (info.st_dev, info.st_ino) in seen
                    or not (directory or stat.S_ISREG(info.st_mode))
                    or (not directory and info.st_nlink != 1)):
                raise ValueError("unsafe or aliased preparation output")
            seen.add((info.st_dev, info.st_ino))
            child = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK
                            | (os.O_DIRECTORY if directory else 0), dir_fd=fd)
            descriptors.append(child)
            opened = os.fstat(child)
            if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
                raise ValueError("preparation entry changed during admission")
            entries.append((fd, name, child, directory))
            if directory: visit(child)
    try:
        root = os.open(output, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        descriptors.append(root)
        info = os.fstat(root)
        if (info.st_dev, info.st_ino) != identity or info.st_uid != os.geteuid():
            raise ValueError("owned output changed before sealing")
        visit(root)
        # Admit every entry before mutating any inode; aliases refuse unchanged.
        for parent, name, child, directory in reversed(entries):
            info = os.fstat(child)
            current = os.stat(name, dir_fd=parent, follow_symlinks=False)
            if (directory_identity(output) != identity
                    or (info.st_dev, info.st_ino) != (current.st_dev, current.st_ino)
                    or (not directory and info.st_nlink != 1)):
                raise ValueError("output ownership changed before chmod")
            os.fchmod(child, 0o500 if directory else 0o400)
        if directory_identity(output) != identity:
            raise ValueError("output ownership changed before publication")
        yield root
        if directory_identity(output) != identity:
            raise ValueError("output ownership changed during publication")
        os.fsync(root)
        os.fchmod(root, 0o500)
    finally:
        for descriptor in reversed(descriptors): os.close(descriptor)


def publish_pending(root):
    # A same-directory link is atomic, refuses replacement, and uses the held
    # directory rather than reopening its possibly replaced pathname.
    pending, target = "t22-input-manifest.pending", "t22-input-manifest.tsv"
    os.link(pending, target, src_dir_fd=root, dst_dir_fd=root, follow_symlinks=False)
    os.unlink(pending, dir_fd=root)
