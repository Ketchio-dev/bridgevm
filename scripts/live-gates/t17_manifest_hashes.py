"""Content identities for sealed T17 assets, including bundle-local symlinks."""
import hashlib
import os
from pathlib import Path
import stat


def file_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tree_hash(root: Path, *, allow_symlinks: bool = True) -> str:
    records: list[bytes] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix()
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            if not allow_symlinks:
                raise ValueError(f"guest payload contains a symlink: {relative}")
            target = os.readlink(path)
            if os.path.isabs(target) or root.resolve() not in path.resolve().parents:
                raise ValueError(f"app bundle symlink escapes its root: {relative}")
            records.append(f"L\t{relative}\t{target}\n".encode())
        elif stat.S_ISREG(mode):
            executable = "1" if mode & 0o111 else "0"
            records.append(f"F\t{relative}\t{executable}\t{file_hash(path)}\n".encode())
        elif stat.S_ISDIR(mode):
            records.append(f"D\t{relative}\n".encode())
        else:
            raise ValueError(f"unsupported app bundle entry: {relative}")
    return hashlib.sha256(b"".join(records)).hexdigest()
