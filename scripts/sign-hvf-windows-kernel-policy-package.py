#!/usr/bin/env python3
"""Create an Ed25519 provenance envelope around a finalized driver package."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kernel_policy_openssl import resolve as resolve_openssl
from kernel_policy_sign_self_test import self_test

ATTESTATION = "bridgevm-kernel-policy-attestation.json"
SIGNATURE = "bridgevm-kernel-policy-attestation.sig"
REPORT = "bridgevm-finalization-report.txt"
POLICY = "windows-kernel-policy"
TOKEN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
HASH = re.compile(r"^[0-9a-f]{64}$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
MAX_FILES = 64
MAX_FILE_BYTES = 256 * 1024 * 1024
MAX_PACKAGE_BYTES = 512 * 1024 * 1024


class Refusal(ValueError):
    pass


def parse_time(text: str) -> dt.datetime:
    if not DATE.fullmatch(text):
        raise Refusal(f"timestamp is not canonical UTC seconds: {text}")
    try:
        return dt.datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)
    except ValueError as exc:
        raise Refusal(f"invalid timestamp: {text}") from exc


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_report(path: Path) -> dict[str, str]:
    data = path.read_bytes()
    if len(data) > 64 * 1024:
        raise Refusal("finalization report is too large")
    try:
        text = data.decode("ascii")
    except UnicodeDecodeError as exc:
        raise Refusal("finalization report is not ASCII") from exc
    fields: dict[str, str] = {}
    for line in text.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if not key or key in fields:
            raise Refusal(f"duplicate or empty finalization field: {key}")
        fields[key] = value
    required = {
        "finalization_complete": "true",
        "signing_mode": "kernel-policy",
        "test_signing_required": "false",
        "sys_kernel_policy_verified": "true",
        "cat_kernel_policy_verified": "true",
    }
    for key, expected in required.items():
        if fields.get(key) != expected:
            raise Refusal(f"finalization report does not prove {key}={expected}")
    return fields


def private_key_ok(path: Path) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError as exc:
        raise Refusal("private key is missing") from exc
    if not stat.S_ISREG(info.st_mode) or path.is_symlink():
        raise Refusal("private key must be a regular non-symlink file")
    if stat.S_IMODE(info.st_mode) & 0o077:
        raise Refusal("private key permissions must deny group and other access")


def source_files(source: Path) -> list[Path]:
    try:
        root = source.lstat()
    except FileNotFoundError as exc:
        raise Refusal("source package is missing") from exc
    if not stat.S_ISDIR(root.st_mode) or source.is_symlink():
        raise Refusal("source package must be a non-symlink directory")
    files: list[Path] = []
    folded: set[str] = set()
    for entry in os.scandir(source):
        name = entry.name
        if not TOKEN.fullmatch(name) or name in (ATTESTATION, SIGNATURE):
            raise Refusal(f"invalid or reserved package entry: {name}")
        if not entry.is_file(follow_symlinks=False) or entry.is_symlink():
            raise Refusal(f"package must be flat regular files: {name}")
        lowered = name.lower()
        if lowered in folded:
            raise Refusal(f"case-colliding package entry: {name}")
        folded.add(lowered)
        files.append(Path(entry.path))
    files.sort(key=lambda item: item.name)
    if not files or len(files) > MAX_FILES or REPORT not in {path.name for path in files}:
        raise Refusal("package file count or finalization report is invalid")
    for suffix in (".inf", ".sys", ".cat"):
        if not any(path.name.lower().endswith(suffix) for path in files):
            raise Refusal(f"package has no {suffix} artifact")
    return files


def copy_regular(source: Path, destination: Path) -> None:
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    source_fd = os.open(source, flags)
    try:
        info = os.fstat(source_fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > MAX_FILE_BYTES:
            raise Refusal(f"source artifact is invalid or too large: {source.name}")
        destination_fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try:
            while True:
                chunk = os.read(source_fd, 1024 * 1024)
                if not chunk:
                    break
                view = memoryview(chunk)
                while view:
                    written = os.write(destination_fd, view)
                    view = view[written:]
            os.fsync(destination_fd)
        finally:
            os.close(destination_fd)
    finally:
        os.close(source_fd)


def canonical_json(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode("ascii")


def public_raw(openssl: str, private_key: Path) -> bytes:
    completed = subprocess.run(
        [openssl, "pkey", "-in", str(private_key), "-pubout", "-outform", "DER"],
        check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    prefix = bytes.fromhex("302a300506032b6570032100")
    if len(completed.stdout) != len(prefix) + 32 or not completed.stdout.startswith(prefix):
        raise Refusal("private key is not an Ed25519 key")
    return completed.stdout[len(prefix):]


def sign_package(
    source: Path,
    output: Path,
    private_key: Path,
    key_id: str,
    package_id: str,
    issued_at: str,
    expires_at: str,
    openssl: str,
) -> dict[str, str]:
    if not TOKEN.fullmatch(key_id) or not TOKEN.fullmatch(package_id):
        raise Refusal("key id and package id must be bounded ASCII tokens")
    issued = parse_time(issued_at)
    expires = parse_time(expires_at)
    if issued >= expires or expires - issued > dt.timedelta(days=366):
        raise Refusal("attestation validity must be positive and at most 366 days")
    private_key_ok(private_key)
    raw_public_key = public_raw(openssl, private_key)
    files = source_files(source)
    output_parent = output.parent
    output_parent.mkdir(parents=True, exist_ok=True)
    if output.exists() or output.is_symlink():
        raise Refusal("signed output already exists")
    temporary = Path(tempfile.mkdtemp(prefix=f".{output.name}.signing.", dir=output_parent))
    os.chmod(temporary, 0o700)
    try:
        total = 0
        for path in files:
            copy_regular(path, temporary / path.name)
            total += (temporary / path.name).stat().st_size
            if total > MAX_PACKAGE_BYTES:
                raise Refusal("package exceeds the signed size bound")
        inventory = [
            {"fileName": path.name, "sha256": sha256(temporary / path.name)}
            for path in files
        ]
        report = parse_report(temporary / REPORT)
        for artifact in inventory:
            if artifact["fileName"] == REPORT:
                continue
            key = f"sha256.{artifact['fileName']}"
            if report.get(key) != artifact["sha256"]:
                raise Refusal(f"report hash does not bind {artifact['fileName']}")
        attestation = {
            "artifacts": inventory,
            "expiresAt": expires_at,
            "issuedAt": issued_at,
            "keyID": key_id,
            "packageID": package_id,
            "policy": POLICY,
            "schemaVersion": 1,
        }
        attestation_path = temporary / ATTESTATION
        attestation_path.write_bytes(canonical_json(attestation))
        os.chmod(attestation_path, 0o600)
        signature_path = temporary / SIGNATURE
        subprocess.run(
            [openssl, "pkeyutl", "-sign", "-rawin", "-inkey", str(private_key),
             "-in", str(attestation_path), "-out", str(signature_path)],
            check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        if signature_path.stat().st_size != 64:
            raise Refusal("Ed25519 signature is not exactly 64 bytes")
        os.chmod(signature_path, 0o600)
        with tempfile.NamedTemporaryFile() as public_file:
            subprocess.run(
                [openssl, "pkey", "-in", str(private_key), "-pubout", "-out", public_file.name],
                check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            subprocess.run(
                [openssl, "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey", public_file.name,
                 "-sigfile", str(signature_path), "-in", str(attestation_path)],
                check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
        os.rename(temporary, output)
        return {
            "attestation_sha256": sha256(output / ATTESTATION),
            "public_key_sha256": hashlib.sha256(raw_public_key).hexdigest(),
        }
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise



def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--private-key", type=Path)
    parser.add_argument("--key-id", default="bridgevm-kernel-policy-2026-01")
    parser.add_argument("--package-id")
    parser.add_argument("--issued-at")
    parser.add_argument("--expires-at")
    parser.add_argument("--openssl")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:  # see kernel_policy_openssl: bare PATH openssl may be LibreSSL
        openssl = resolve_openssl(args.openssl or "openssl", args.openssl is not None)
    except RuntimeError as error:
        raise SystemExit(f"REFUSED: {error}")
    if args.self_test:
        self_test(openssl, sign_package, Refusal, REPORT)
        return 0
    required = (args.source, args.out, args.private_key, args.package_id)
    if any(value is None for value in required):
        parser.error("--source, --out, --private-key and --package-id are required")
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    issued_at = args.issued_at or now.strftime("%Y-%m-%dT%H:%M:%SZ")
    expires_at = args.expires_at or (now + dt.timedelta(days=365)).strftime("%Y-%m-%dT%H:%M:%SZ")
    try:
        result = sign_package(
            args.source, args.out, args.private_key, args.key_id, args.package_id,
            issued_at, expires_at, openssl,
        )
    except (OSError, subprocess.CalledProcessError, Refusal) as exc:
        print(f"REFUSED: {exc}", file=os.sys.stderr)
        return 1
    print(f"signed_package={args.out}")
    print(f"attestation_sha256={result['attestation_sha256']}")
    print(f"public_key_sha256={result['public_key_sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
