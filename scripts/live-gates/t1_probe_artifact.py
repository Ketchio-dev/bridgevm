"""T1 build provenance; importing this module executes no commands."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def snapshot(path, limit=32 * 1024 * 1024):
    path = Path(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.getuid() or not 0 < before.st_size <= limit:
            raise ValueError('T1 input is not bounded owned regular data')
        data = b''
        while len(data) <= limit:
            block = os.read(fd, min(65536, limit + 1 - len(data)))
            if not block:break
            data += block
        after = os.fstat(fd)
    finally:os.close(fd)
    fields = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if len(data) != before.st_size or fields(before) != fields(after) or fields(after) != fields(path.stat(follow_symlinks=False)):
        raise ValueError('T1 input changed')
    return data, hashlib.sha256(data).hexdigest()


def load(path):
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:raise ValueError('duplicate T1 field')
            value[key] = item
        return value
    return json.loads(snapshot(path, 65536)[0], object_pairs_hook=pairs)


def save_new(path, value):
    data = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, 'wb', closefd=False) as out:out.write(data);out.flush();os.fsync(fd)
        os.fchmod(fd, 0o400)
    finally:os.close(fd)
    parent = os.open(Path(path).parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:os.fsync(parent)
    finally:os.close(parent)


def command(argv):
    return subprocess.run(argv, cwd=ROOT, capture_output=True, check=True, timeout=30).stdout


def source():
    if command(['git', 'status', '--porcelain']).strip():
        raise ValueError('T1 source has tracked changes')
    sha = command(['git', 'rev-parse', 'HEAD']).decode().strip()
    tree = command(['git', 'rev-parse', 'HEAD^{tree}']).decode().strip()
    if any(len(s) != 40 or any(c not in '0123456789abcdef' for c in s) for s in [sha, tree]):
        raise ValueError('T1 source identity is malformed')
    return sha, tree


def prepare(binary, built, before):
    binary = Path(binary)
    sha, tree = source()
    if sha != before or not os.access(binary, os.X_OK):raise ValueError('T1 build source or executable differs')
    snapshot(binary)
    if built:command(['codesign', '--sign', '-', '--entitlements', str(ROOT / 'apps/macos/HvfRunner.entitlements'), '--force', str(binary)])
    command(['codesign', '--verify', '--strict', str(binary)])
    entitlements = plistlib.loads(command(['codesign', '-d', '--entitlements', '-', '--xml', str(binary)]))
    if entitlements.get('com.apple.security.hypervisor') is not True:raise ValueError('T1 hypervisor entitlement is not true')
    digest = snapshot(binary)[1]
    path = binary.with_name(binary.name + '.' + sha + '.' + digest + '.t1-build.json')
    value = {'schema_version': 1, 'binary_hash': digest, 'binary_source_commit': sha,
             'source_tree': tree, 'binary_profile': 'debug', 'binary_features': '',
             'rust_toolchain': command(['rustc', '--version']).decode().strip(),
             'artifact_signing_class': 'ad-hoc', 'strict_signature_verified': True,
             'hypervisor_entitlement_true': True}
    if built:
        if path.exists():
            if load(path) != value:raise ValueError('T1 cached build seal differs')
        else:save_new(path, value)
    elif load(path) != value:
        raise ValueError('T1 --skip-build needs the matching previously built source seal')
    if source() != (sha, tree) or snapshot(binary)[1] != digest:raise ValueError('T1 artifact changed during preparation')
    return path


if __name__ == '__main__':
    try:
        mode = sys.argv[1]
        if mode in ['target', 'build']:
            from t1_probe_build import build, target
            print(target() if mode == 'target' else build())
        elif mode == 'source':print(source()[0])
        elif mode == 'prepare':print(prepare(sys.argv[2], sys.argv[3] == '1', sys.argv[4]))
        else:raise ValueError('unknown T1 artifact command')
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        raise SystemExit(f'T1 artifact refused: {error}') from None
