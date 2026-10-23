#!/usr/bin/env python3
"""Synthetic packaged BridgeVMControl for the vTPM lifecycle commands T17 cleanup runs.

Keys live in a file keychain, fake-vtpm-keychain beside the app bundle, as one
<account>.key per item. A per-account marker makes a command fail like the
product would: .refuse (forget-import reports a key mismatch), .unreadable
(every read fails), .silent (forget-import exits 0 with no line and no
deletion) and .hang (the command never returns). calls.jsonl records every
invocation. Options, refusals and messages follow VTPMLifecycleCommand,
VTPMImportCleanupCommand and VTPMIdentityLifecycle.
"""
import hashlib
import json
import os
from pathlib import Path
import secrets
import stat
import sys
import time

KEYCHAIN = Path(__file__).resolve().parents[2].parent / "fake-vtpm-keychain"
OPTIONS = ("--stable-vm-id", "--state-dir", "--package", "--recovery-code-file")
FORMAT = "bridgevm-vtpm-recovery-v1"
MISSING_KEY = "기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. 복구 키를 복원하거나 명시적으로 TPM을 재설정하세요."
UNREADABLE = "Keychain에서 vTPM 상태 키를 읽지 못했습니다 (OSStatus -25293)."
MISMATCH = "현재 vTPM 키가 선택한 복구 패키지와 일치하지 않습니다."
INVALID_PACKAGE = "vTPM 복구 패키지가 손상되었거나 코드와 일치하지 않습니다."


class Refused(Exception):
    pass


def marked(account, name):
    return (KEYCHAIN / f"{account}.{name}").exists()


def stored_key(account):
    if marked(account, "unreadable"):
        raise Refused(UNREADABLE)
    try:
        return bytes.fromhex((KEYCHAIN / f"{account}.key").read_text())
    except FileNotFoundError:
        raise Refused(MISSING_KEY) from None


def fingerprint(state):
    digest = hashlib.sha256(b"bridgevm-vtpm-state-v1\0")
    if not os.path.lexists(state):
        return digest.hexdigest()
    if state.is_symlink() or not state.is_dir():
        raise Refused(f"vTPM 상태에 안전하지 않은 링크 또는 파일이 있습니다: {state}")
    for path in sorted(state.rglob("*"), key=str):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise Refused(f"vTPM 상태에 안전하지 않은 링크 또는 파일이 있습니다: {path}")
        if path.is_file() and path.name != ".lock":
            digest.update(str(path.relative_to(state)).encode() + b"\0" + path.read_bytes() + b"\xff")
    return digest.hexdigest()


def sealed(key, code):
    return bytes(a ^ b for a, b in zip(key, hashlib.sha256(code.encode()).digest()))


def create_private(path, data):
    with os.fdopen(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600), "wb") as output:
        output.write(data)


def export(stable, state, package, code_file):
    for output in (package, code_file):
        if os.path.lexists(output):
            raise Refused(f"refusing to overwrite existing output: {output}")
    if not state.exists():
        raise Refused(f"required input is missing: {state}")
    key, code = stored_key(stable), secrets.token_urlsafe(32)
    state_fingerprint = fingerprint(state)
    create_private(package, json.dumps({"format": FORMAT, "stableVMID": stable, "stateFingerprint": state_fingerprint,
                                        "sealed": sealed(key, code).hex()}).encode())
    create_private(code_file, (code + "\n").encode())
    print(f"exported_package={package}\nrecovery_code_file={code_file}\nstate_fingerprint={state_fingerprint}")


def bounded(path, limit, owner_only=False):
    try:
        info = os.stat(path, follow_symlinks=False)
    except FileNotFoundError:
        raise Refused(f"required input is missing: {path}") from None
    if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= limit or (owner_only and info.st_mode & 0o077):
        raise Refused(f"required input is missing: {path}")


def forget(stable, state, package, code_file):
    if marked(stable, "silent"):
        return
    bounded(package, 1_048_576)
    bounded(code_file, 4_096, owner_only=True)
    try:
        value = json.loads(package.read_text())
        expected = sealed(bytes.fromhex(value["sealed"]), code_file.read_text().strip())
        valid = value["format"] == FORMAT and value["stateFingerprint"] == fingerprint(state)
    except (ValueError, KeyError, TypeError):
        valid = False
    if not valid:
        raise Refused(INVALID_PACKAGE)
    if stored_key(stable) != expected or marked(stable, "refuse"):
        raise Refused(MISMATCH)
    (KEYCHAIN / f"{stable}.key").unlink()
    print(f"forgot_imported_key={stable}")


def main(arguments):
    KEYCHAIN.mkdir(exist_ok=True)
    with (KEYCHAIN / "calls.jsonl").open("a", encoding="utf-8") as log:
        log.write(json.dumps(arguments) + "\n")
    if arguments[:1] != ["--vtpm-lifecycle"] or arguments[1:2] not in (["export"], ["forget-import"]):
        return 0
    values = dict(zip(arguments[2::2], arguments[3::2]))
    try:
        if len(arguments) != 10 or set(values) != set(OPTIONS) or not all(values[o].startswith("/") for o in OPTIONS[1:]):
            raise Refused(f"{arguments[1]} requires four exact options")
        if marked(values["--stable-vm-id"], "hang"):
            time.sleep(3600)
        command = export if arguments[1] == "export" else forget
        command(values["--stable-vm-id"], *(Path(values[option]) for option in OPTIONS[1:]))
    except Refused as error:
        sys.stderr.write(f"ERROR: {error}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
