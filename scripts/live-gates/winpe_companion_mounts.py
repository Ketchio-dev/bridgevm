"""Bounded, exact-backing ownership for private D4 read-only inspection."""
from contextlib import contextmanager
from pathlib import Path
import os
import plistlib
import re
import signal
import subprocess


class MountSafetyError(RuntimeError):
    def __init__(self, message, *, cleanup_complete=False):
        super().__init__(message)
        self.cleanup_complete = cleanup_complete is True


def _command(arguments, timeout):
    return subprocess.run(["hdiutil", *arguments], stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=timeout, check=False)


def _entities(value, mount_root):
    if not isinstance(value, list) or not value or any(not isinstance(x, dict) for x in value):
        raise ValueError("invalid-attachment-entities")
    roots = [x.get("dev-entry") for x in value
             if re.fullmatch(r"/dev/disk[0-9]+", str(x.get("dev-entry", "")))]
    if len(roots) != 1:
        raise ValueError("ambiguous-attachment-device")
    device = roots[0]
    for item in value:
        child = item.get("dev-entry")
        if child is not None and (not isinstance(child, str) or not re.fullmatch(
                re.escape(device) + r"(?:s[0-9]+)*", child)):
            raise ValueError("foreign-attachment-device")
        mount = item.get("mount-point")
        if mount is not None:
            if not isinstance(mount, str) or not mount or not Path(mount).is_absolute():
                raise ValueError("invalid-attachment-mount")
            if not Path(mount).resolve().is_relative_to(mount_root):
                raise ValueError("attachment-outside-owned-root")
    return device, value


def _snapshot(image, mount_root):
    result = _command(["info", "-plist"], 10)
    if result.returncode != 0:
        raise ValueError("mount-inventory-failed")
    value = plistlib.loads(result.stdout)
    images = value.get("images") if isinstance(value, dict) else None
    if not isinstance(images, list):
        raise ValueError("invalid-mount-inventory")
    matches, devices = [], set()
    for item in images:
        if not isinstance(item, dict) or not isinstance(item.get("image-path"), str):
            raise ValueError("unidentified-mount-inventory-entry")
        path = item["image-path"]
        entries = item.get("system-entities")
        if not path or not Path(path).is_absolute() or not isinstance(entries, list):
            raise ValueError("invalid-mount-inventory-entry")
        if any(not isinstance(entity, dict) for entity in entries):
            raise ValueError("invalid-mount-inventory-entities")
        devices.update(entity["dev-entry"] for entity in entries
                       if isinstance(entity.get("dev-entry"), str))
        if Path(path).resolve() == image:
            matches.append(_entities(entries, mount_root))
    if len(matches) > 1:
        raise ValueError("ambiguous-backing-attachment")
    return (matches[0] if matches else None), devices


def _cleanup(image, mount_root, expected_device):
    attachment, _ = _snapshot(image, mount_root)
    device = expected_device
    detached = True
    if attachment is not None:
        current, _ = attachment
        if device is not None and current != device:
            return False
        device = current
        detached = _command(["detach", device], 20).returncode == 0
    remaining, devices = _snapshot(image, mount_root)
    return detached and remaining is None and (device is None or device not in devices)


def _interrupt(signum, _frame):
    raise InterruptedError("mount-operation-cancelled-%d" % signum)


@contextmanager
def mounted_image(image, mount_root):
    """Yield entities only for a fresh private attachment; fail closed on exit."""
    image, mount_root = Path(image), Path(mount_root)
    previous = {}
    created = attempted = uncertain = cleanup_complete = False
    identity = device = error = None
    try:
        image = image.resolve(strict=True)
        if not image.is_file():
            raise ValueError("inspection-image-not-file")
        mount_root = mount_root.parent.resolve(strict=True) / mount_root.name
        existing, _ = _snapshot(image, mount_root)
        if existing is not None:
            raise ValueError("inspection-image-already-attached")
        mount_root.mkdir(mode=0o700, exist_ok=False)
        created = True
        info = mount_root.lstat()
        identity = (info.st_dev, info.st_ino)
        for sig in (signal.SIGTERM, signal.SIGINT):
            previous[sig] = signal.signal(sig, _interrupt)
        attempted = True
        try:
            result = _command(["attach", "-imagekey", "diskimage-class=CRawDiskImage",
                               "-readonly", "-nobrowse", "-plist", "-mountroot",
                               str(mount_root), str(image)], 60)
        except BaseException:
            uncertain = True
            raise
        if result.returncode != 0:
            raise ValueError("image-attach-failed")
        parsed = plistlib.loads(result.stdout)
        device, _ = _entities(parsed.get("system-entities") if isinstance(parsed, dict)
                              else None, mount_root)
        observed, _ = _snapshot(image, mount_root)
        if observed is None or observed[0] != device:
            raise ValueError("attachment-ownership-not-confirmed")
        yield observed[1]
    except BaseException as exc:
        error = exc
    finally:
        try:
            for sig in previous:
                signal.signal(sig, signal.SIG_IGN)
            if created:
                cleanup_complete = (not attempted or _cleanup(image, mount_root, device))
                cleanup_complete = cleanup_complete and not uncertain
                if cleanup_complete:
                    info = mount_root.lstat()
                    if mount_root.is_symlink() or (info.st_dev, info.st_ino) != identity:
                        cleanup_complete = False
                    else:
                        os.rmdir(mount_root)
        except BaseException:
            cleanup_complete = False
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
    if error is not None or not cleanup_complete:
        raise MountSafetyError("owned-mount-operation-failed", cleanup_complete=cleanup_complete) from error
