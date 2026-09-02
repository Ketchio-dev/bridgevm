#!/usr/bin/env python3
"""Strictly authenticate the private B7 playback-and-shutdown inputs."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path

SCHEMA = "bridgevm.b7-audio-teardown-input.v1"
PROFILE = "windows-hda-coreaudio-playback-shutdown-v1"
ASSETS = ("image", "vars", "binary", "firmware", "playback_script")
FIXED = {
    "schema": SCHEMA,
    "profile": PROFILE,
    "sample_count": "10",
    "ram_mib": "6144",
    "smp_cpus": "4",
    "sample_rate_hz": "48000",
    "tone_hz": "440",
    "tone_seconds": "2",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")


class ManifestError(ValueError):
    pass


def seal(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse(path: Path) -> tuple[dict[str, str], dict[str, tuple[str, str]]]:
    if not path.is_file() or path.is_symlink() or not 1 <= path.stat().st_size <= 16_384:
        raise ManifestError("manifest is missing, unsafe, empty, or over 16 KiB")
    metadata: dict[str, str] = {}
    assets: dict[str, tuple[str, str]] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) == 2 and fields[0] in FIXED and fields[0] not in metadata:
            metadata[fields[0]] = fields[1]
        elif len(fields) == 3 and fields[0] in ASSETS and fields[0] not in assets:
            raw, expected = fields[1:]
            if not SHA256.fullmatch(expected):
                raise ManifestError(f"asset digest is malformed on row {number}")
            assets[fields[0]] = raw, expected
        else:
            raise ManifestError(f"unknown, duplicate, or malformed row {number}")
    if metadata != FIXED or set(assets) != set(ASSETS):
        raise ManifestError("manifest does not contain its exact fixed contract")
    for key in ASSETS[:-1]:
        raw = assets[key][0]
        if not raw.startswith("/") or raw != os.path.normpath(raw):
            raise ManifestError(f"{key} path is not normalized and absolute")
    if assets["playback_script"][0] != "scripts/win-assets/bv-audio-teardown.ps1":
        raise ManifestError("playback_script is not the fixed worktree resource")
    if assets["image"][0] == assets["vars"][0]:
        raise ManifestError("image and vars paths alias")
    return metadata, assets


def verify(path: Path, repo: Path, sealed_binary: Path | None = None) -> dict:
    try:
        metadata, assets = parse(path)
        verified: dict[str, dict[str, str]] = {}
        for key, (raw, expected) in assets.items():
            if key == "playback_script":
                candidate = repo / raw
            elif key == "binary" and sealed_binary is not None:
                candidate = sealed_binary
            else:
                candidate = Path(raw)
            if not candidate.is_file() or candidate.is_symlink():
                raise ManifestError(f"{key} is missing, non-regular, or a symlink")
            actual = seal(candidate)
            if actual != expected:
                raise ManifestError(f"{key} hash mismatch")
            verified[key] = {"path": str(candidate), "sha256": actual}
        return {"valid": True, "failure_code": "none", "metadata": metadata, "assets": verified}
    except (OSError, UnicodeError, ManifestError) as error:
        return {"valid": False, "failure_code": "invalid-input", "detail": str(error), "metadata": {}, "assets": {}}


def self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory(prefix="bridgevm-b7-manifest-") as temporary:
        root = Path(temporary)
        repo = root / "repo"
        script = repo / "scripts/win-assets/bv-audio-teardown.ps1"
        script.parent.mkdir(parents=True)
        script.write_bytes(b"fixture\r\n")
        assets = {}
        for key in ASSETS[:-1]:
            candidate = root / key
            candidate.write_bytes((key + "\n").encode())
            assets[key] = candidate
        manifest = root / "manifest.tsv"
        rows = [f"{key}\t{value}" for key, value in FIXED.items()]
        rows += [f"{key}\t{assets[key]}\t{seal(assets[key])}" for key in ASSETS[:-1]]
        rows += [f"playback_script\tscripts/win-assets/bv-audio-teardown.ps1\t{seal(script)}"]
        manifest.write_text("\n".join(rows) + "\n", encoding="utf-8")
        assert verify(manifest, repo)["valid"] is True
        manifest.write_text(manifest.read_text() + rows[0] + "\n")
        assert verify(manifest, repo)["valid"] is False
        manifest.write_text("\n".join(rows) + "\n")
        assets["vars"].write_bytes(b"changed\n")
        assert verify(manifest, repo)["valid"] is False
        assets["vars"].write_bytes(b"vars\n")
        link = root / "linked-binary"
        link.symlink_to(assets["binary"])
        assert verify(manifest, repo, link)["valid"] is False
    print("PASS: B7 audio teardown manifest self-test")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--repo", type=Path)
    parser.add_argument("--sealed-binary", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.manifest is None or args.repo is None or args.out is None:
        parser.error("--manifest, --repo and --out are required")
    result = verify(args.manifest, args.repo.resolve(), args.sealed_binary)
    try:
        with args.out.open("x", encoding="utf-8") as output:
            json.dump(result, output, indent=2, sort_keys=True)
            output.write("\n")
    except OSError as error:
        print(f"B7 manifest output refused: {error}", file=sys.stderr)
        return 2
    if not result["valid"]:
        print(f"B7 manifest refused: {result.get('detail', 'invalid input')}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
