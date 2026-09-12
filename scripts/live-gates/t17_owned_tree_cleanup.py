#!/usr/bin/env python3
"""Remove only an identity-bound, validated T17 temporary tree."""
import argparse
import os
import re
import stat
import sys


DIRECTORY_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
MAXIMUM_ENTRIES = 100_000
MAXIMUM_DEPTH = 128


def identity(info):
    return info.st_dev, info.st_ino, info.st_uid, stat.S_IFMT(info.st_mode)


def check_node(info, device, owner):
    if info.st_dev != device or info.st_uid != owner:
        raise ValueError("cleanup node changed device or owner")
    if stat.S_ISDIR(info.st_mode):
        return
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        raise ValueError("cleanup accepts only directories and single-link regular files")


def check_identity(info, expected, device, owner):
    check_node(info, device, owner)
    if identity(info) != expected:
        raise ValueError("cleanup node identity changed")


def inspect_tree(descriptor, device, owner, remaining, depth=0):
    if depth > MAXIMUM_DEPTH:
        raise ValueError("cleanup directory depth exceeds its bound")
    result = {}
    for name in sorted(os.listdir(descriptor)):
        remaining[0] -= 1
        if remaining[0] < 0:
            raise ValueError("cleanup entry count exceeds its bound")
        info = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
        check_node(info, device, owner)
        expected = identity(info)
        children = None
        if stat.S_ISDIR(info.st_mode):
            child = os.open(name, DIRECTORY_FLAGS, dir_fd=descriptor)
            try:
                check_identity(os.fstat(child), expected, device, owner)
                children = inspect_tree(child, device, owner, remaining, depth + 1)
            finally:
                os.close(child)
        result[name] = expected, children
    return result


def remove_contents(descriptor, expected, children, device, owner):
    info = os.fstat(descriptor)
    check_identity(info, expected, device, owner)
    if set(os.listdir(descriptor)) != set(children):
        raise ValueError("cleanup directory entries changed after validation")
    # Copied immutable payloads use 0500 directories and 0400 files. Unlinking
    # requires directory write/search access, not writable file contents.
    os.fchmod(descriptor, stat.S_IMODE(info.st_mode) | stat.S_IWUSR | stat.S_IXUSR)
    for name, (child_identity, descendants) in children.items():
        info = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
        check_identity(info, child_identity, device, owner)
        if descendants is None:
            os.unlink(name, dir_fd=descriptor)
            continue
        child = os.open(name, DIRECTORY_FLAGS, dir_fd=descriptor)
        try:
            check_identity(os.fstat(child), child_identity, device, owner)
            remove_contents(child, child_identity, descendants, device, owner)
            info = os.stat(name, dir_fd=descriptor, follow_symlinks=False)
            check_identity(info, child_identity, device, owner)
            os.rmdir(name, dir_fd=descriptor)
        finally:
            os.close(child)


def cleanup(root, job_id, captured_identity):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", job_id):
        raise ValueError("invalid cleanup job identity")
    pattern = r"/tmp/bridgevm-e2e-" + re.escape(job_id) + r"\.[A-Za-z0-9]{6}"
    if not re.fullmatch(pattern, root):
        raise ValueError("cleanup root is outside the exact job boundary")
    if not re.fullmatch(r"[0-9]+:[0-9]+", captured_identity):
        raise ValueError("cleanup root lacks a captured device/inode identity")
    device, inode = map(int, captured_identity.split(":"))
    owner = os.geteuid()
    # /tmp itself is the platform-owned alias on macOS. Do not resolve or
    # follow any link inside it, including the caller-supplied root entry.
    parent = os.open("/tmp", os.O_RDONLY | os.O_DIRECTORY)
    try:
        name = os.path.basename(root)
        info = os.stat(name, dir_fd=parent, follow_symlinks=False)
        check_node(info, device, owner)
        if not stat.S_ISDIR(info.st_mode) or info.st_ino != inode:
            raise ValueError("cleanup root does not match its captured identity")
        expected = identity(info)
        descriptor = os.open(name, DIRECTORY_FLAGS, dir_fd=parent)
        try:
            check_identity(os.fstat(descriptor), expected, device, owner)
            children = inspect_tree(descriptor, device, owner, [MAXIMUM_ENTRIES])
            remove_contents(descriptor, expected, children, device, owner)
            info = os.stat(name, dir_fd=parent, follow_symlinks=False)
            check_identity(info, expected, device, owner)
            os.rmdir(name, dir_fd=parent)
            try:
                os.stat(name, dir_fd=parent, follow_symlinks=False)
            except FileNotFoundError:
                return
            raise ValueError("cleanup root still exists after removal")
        finally:
            os.close(descriptor)
    finally:
        os.close(parent)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--job-id", required=True)
    parser.add_argument("--identity", required=True)
    args = parser.parse_args()
    try:
        cleanup(args.root, args.job_id, args.identity)
    except (OSError, ValueError) as error:
        print(f"T17 owned-tree cleanup refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
