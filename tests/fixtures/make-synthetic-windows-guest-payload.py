#!/usr/bin/env python3
"""Create a synthetic signed ARM64 payload for deterministic verifier tests."""
from __future__ import annotations

import argparse
import hashlib
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path


ROLES = ("storage", "serial", "network")


def pe(machine: int = 0xAA64) -> bytes:
    data = bytearray(512)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
    return bytes(data)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def create(payload: Path, manifest: Path, openssl: str) -> None:
    payload.mkdir(parents=True)
    with tempfile.TemporaryDirectory() as temporary:
        signing = Path(temporary)
        content = signing / "catalog-content"
        key = signing / "key.pem"
        certificate = signing / "certificate.pem"
        catalog = signing / "driver.cat"
        content.write_bytes(b"synthetic BridgeVM catalog fixture\n")
        subprocess.run(
            [openssl, "req", "-x509", "-newkey", "rsa:2048", "-nodes",
             "-subj", "/CN=BridgeVM deterministic fixture/", "-days", "1",
             "-keyout", str(key), "-out", str(certificate)],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        subprocess.run(
            [openssl, "cms", "-sign", "-binary", "-nodetach", "-nosmimecap", "-econtent_type", "1.3.6.1.4.1.311.10.1",
             "-in", str(content), "-signer", str(certificate), "-inkey", str(key),
             "-outform", "DER", "-out", str(catalog)],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        for role in ROLES:
            directory = payload / role
            directory.mkdir()
            (directory / f"{role}.inf").write_bytes(
                (f"[Version]\r\nSignature=\"$Windows NT$\"\r\n"
                 f"[Manufacturer]\r\nBridgeVM=Models,NTARM64\r\n").encode()
            )
            (directory / f"{role}.sys").write_bytes(pe())
            shutil.copyfile(catalog, directory / f"{role}.cat")

    lines = ["schema\tbridgevm-windows-guest-payload-v1", "architecture\tarm64"]
    for role in ROLES:
        lines.append(
            f"driver\t{role}\t{role}/{role}.inf\t{role}/{role}.cat\t{role}/{role}.sys"
        )
    for path in sorted(payload.rglob("*")):
        if path.is_file():
            lines.append(f"file\t{path.relative_to(payload).as_posix()}\t{digest(path)}")
    manifest.write_bytes(("\r\n".join(lines) + "\r\n").encode())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("payload", type=Path)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--openssl", default="/usr/bin/openssl")
    args = parser.parse_args()
    if args.payload.exists() or args.manifest.exists():
        parser.error("payload and manifest outputs must not exist")
    create(args.payload, args.manifest, args.openssl)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
