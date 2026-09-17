#!/usr/bin/env python3
"""Strictly authenticate installed-disk import inputs for the physical-Mac tier."""
from __future__ import annotations
import argparse, hashlib, json, os, re, stat, sys
from pathlib import Path

ASSETS = ("app_bundle", "app_executable", "product_helper", "runner",
          "source_disk", "source_vars", "source_vtpm", "source_vtpm_package", "source_vtpm_code")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
RELATIONS = {
    "app_executable": "Contents/MacOS/BridgeVMControl",
    "product_helper": "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E",
    "runner": "Contents/Resources/target/release/hvf-runner",
}

class ManifestError(ValueError):
    pass

def file_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def tree_hash(root: Path, *, allow_symlinks: bool) -> str:
    records: list[bytes] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        relative = path.relative_to(root).as_posix(); mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            if not allow_symlinks:
                raise ManifestError(f"source tree contains a symlink: {relative}")
            target = os.readlink(path)
            if os.path.isabs(target) or root.resolve() not in path.resolve().parents:
                raise ManifestError(f"app bundle symlink escapes its root: {relative}")
            records.append(f"L\t{relative}\t{target}\n".encode())
        elif stat.S_ISREG(mode):
            executable = "1" if mode & 0o111 else "0"
            records.append(f"F\t{relative}\t{executable}\t{file_hash(path)}\n".encode())
        elif stat.S_ISDIR(mode): records.append(f"D\t{relative}\n".encode())
        else: raise ManifestError(f"unsupported tree entry: {relative}")
    return hashlib.sha256(b"".join(records)).hexdigest()

def parse(path: Path) -> tuple[str, dict[str, tuple[Path, str]]]:
    if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= 16 * 1024:
        raise ManifestError("manifest is missing, unsafe, empty, or oversized")
    mode: str | None = None; assets: dict[str, tuple[Path, str]] = {}
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        if fields[0] == "campaign_mode" and len(fields) == 2 and mode is None:
            mode = fields[1]; continue
        if len(fields) != 3 or fields[0] not in ASSETS or fields[0] in assets:
            raise ManifestError(f"malformed, duplicate, or unknown row {number}")
        raw, expected = fields[1], fields[2]
        if not raw.startswith("/") or os.path.normpath(raw) != raw or not SHA256.fullmatch(expected):
            raise ManifestError(f"invalid path or digest in row {number}")
        assets[fields[0]] = Path(raw), expected
    if mode not in ("pilot", "release") or set(assets) != set(ASSETS):
        raise ManifestError("manifest lacks its one exact mode and asset set")
    bundle = assets["app_bundle"][0]
    for key, relative in RELATIONS.items():
        if assets[key][0] != bundle / relative:
            raise ManifestError(f"{key} is outside its fixed app location")
    return mode, assets

def verify(path: Path) -> dict:
    try: mode, assets = parse(path)
    except (OSError, UnicodeError, ManifestError) as error:
        return {"valid": False, "verified": False, "campaign_mode": "pilot",
                "failure_code": "internal-error", "detail": str(error), "assets": {}}
    public: dict[str, dict[str, str]] = {}
    for key, (candidate, expected) in assets.items():
        directory = key in ("app_bundle", "source_vtpm")
        present = candidate.is_dir() if directory else candidate.is_file()
        if not present or candidate.is_symlink():
            code = {"source_disk": "missing-installed-disk", "source_vars": "missing-vars",
                    "source_vtpm": "missing-vtpm", "source_vtpm_package": "missing-vtpm", "source_vtpm_code": "missing-vtpm"}.get(key, "missing-app-artifact")
            return {"valid": True, "verified": False, "campaign_mode": mode,
                    "failure_code": code, "detail": key, "assets": public}
        if key == "source_vars" and candidate.stat().st_size != 64 * 1024 * 1024:
            return {"valid": True, "verified": False, "campaign_mode": mode,
                    "failure_code": "invalid-vars", "detail": key, "assets": public}
        try:
            actual = tree_hash(candidate, allow_symlinks=key == "app_bundle") if directory else file_hash(candidate)
        except (OSError, ManifestError):
            return {"valid": True, "verified": False, "campaign_mode": mode,
                    "failure_code": "hash-mismatch", "detail": key, "assets": public}
        public[key] = {"path": str(candidate), "sha256": actual}
        if actual != expected:
            return {"valid": True, "verified": False, "campaign_mode": mode,
                    "failure_code": "hash-mismatch", "detail": key, "assets": public}
    return {"valid": True, "verified": True, "campaign_mode": mode,
            "failure_code": "none", "detail": "none", "assets": public}

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True); parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(); result = verify(args.manifest); args.out.parent.mkdir(parents=True, exist_ok=True)
    try:
        with args.out.open("x", encoding="utf-8") as output:
            json.dump(result, output, indent=2, sort_keys=True); output.write("\n")
    except OSError as error:
        print(f"manifest result output refused: {error}", file=sys.stderr); return 2
    if not result["verified"]:
        print(f"A9 import input preflight blocked: {result['failure_code']} ({result['detail']})", file=sys.stderr); return 1
    return 0

if __name__ == "__main__": raise SystemExit(main())
