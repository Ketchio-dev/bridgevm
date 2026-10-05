"""Private D11 files: exclusive writes and stable descriptor-based seals."""
import hashlib
import json
import os
from pathlib import Path
import stat


def identity(info):
    return [info.st_dev, info.st_ino, info.st_mode, info.st_uid, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns]


def canonical(path):
    path = Path(path)
    if not path.is_absolute() or path.resolve() != path:
        raise ValueError("noncanonical fixture path")
    return path


class FileSeal:
    def __init__(self, path, limit, expected=None):
        self.path = canonical(path)
        self.fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        try:
            info = os.fstat(self.fd)
            if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
                    or not 0 < info.st_size <= limit):
                raise ValueError("fixture input type or size refused")
            self.identity = identity(info)
            digest = hashlib.sha256()
            while block := os.read(self.fd, 1024 * 1024):
                digest.update(block)
            self.sha256 = digest.hexdigest()
            self.check()
            if expected is not None and self.sha256 != expected:
                raise ValueError("fixture input seal differs")
        except BaseException:
            self.close()
            raise

    def check(self):
        if (identity(os.fstat(self.fd)) != self.identity
                or identity(self.path.lstat()) != self.identity):
            raise ValueError("fixture file changed")

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None

    def __enter__(self): return self
    def __exit__(self, *args): self.close()


def digest(path, limit=128 << 30):
    with FileSeal(path, limit) as item:
        return item.sha256


def read(path, limit=65536):
    with FileSeal(path, limit) as item:
        os.lseek(item.fd, 0, os.SEEK_SET)
        data = os.read(item.fd, limit + 1)
        item.check()
        if len(data) != item.identity[5]: raise ValueError("short fixture read")
        return data


def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value: raise ValueError("duplicate fixture field")
        value[key] = item
    return value


def document(path):
    return json.loads(read(path), object_pairs_hook=unique,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON")))


def write(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def record(path, value):
    data = (json.dumps(value, sort_keys=True) + "\n").encode()
    write(path, data)
    path.chmod(0o400)
    return hashlib.sha256(data).hexdigest()


def tree_paths(root, directories=None):
    pending, paths = [root], []
    while pending:
        path = pending.pop()
        paths.append(path)
        before = path.lstat()
        if stat.S_ISDIR(before.st_mode):
            if directories is not None: directories.append((path, identity(before)))
            with os.scandir(path) as entries:
                for entry in entries:
                    if len(pending) + len(paths) >= 1024:
                        raise ValueError("fixture tree too large")
                    pending.append(path / entry.name)
            if identity(path.lstat()) != identity(before): raise ValueError("fixture directory changed during inventory")
    return [root, *sorted(paths[1:])]


class TreeSeal:
    """Small helper/payload trees only; no links or ambient dependencies."""
    def __init__(self, path, expected=None):
        self.path, self.files, self.directories = canonical(path), [], []
        try:
            if not stat.S_ISDIR(self.path.lstat().st_mode): raise ValueError("fixture tree is not a directory")
            records = []
            paths = tree_paths(self.path, self.directories)
            total = 0
            for entry in paths:
                info = entry.lstat()
                name = entry.relative_to(self.path).as_posix()
                if "\n" in name or "\t" in name: raise ValueError("unsafe tree name")
                if stat.S_ISDIR(info.st_mode):
                    records.append(f"D\t{name}\n")
                elif stat.S_ISREG(info.st_mode):
                    total += info.st_size
                    if total > 256 << 20: raise ValueError("fixture tree bytes exceed bound")
                    item = FileSeal(entry, 128 << 20)
                    self.files.append(item)
                    records.append(f"F\t{name}\t{int(bool(info.st_mode & 0o111))}\t{item.sha256}\n")
                else:
                    raise ValueError("fixture tree contains nonregular entry")
            self.sha256 = hashlib.sha256("".join(records).encode()).hexdigest()
            self.check()
            if expected is not None and self.sha256 != expected:
                raise ValueError("fixture tree seal differs")
        except BaseException:
            self.close()
            raise

    def check(self):
        for item in self.files: item.check()
        for path, observed in self.directories:
            if identity(path.lstat()) != observed: raise ValueError("fixture directory changed")

    def close(self):
        for item in self.files: item.close()

    def __enter__(self): return self
    def __exit__(self, *args): self.close()
