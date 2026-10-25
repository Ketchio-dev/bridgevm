#!/usr/bin/env python3
"""Strictly authenticate installed-disk import inputs for the physical-Mac tier."""
from __future__ import annotations
import argparse, hashlib, importlib.util, json, os, stat, sys
from pathlib import Path

# The row grammar, loaded by path so that every loader of this script finds it.
_GRAMMAR = importlib.util.spec_from_file_location("t19_manifest_grammar", Path(__file__).with_name("t19_manifest_grammar.py"))
GRAMMAR = importlib.util.module_from_spec(_GRAMMAR); _GRAMMAR.loader.exec_module(GRAMMAR)
ASSETS, ManifestError, parse = GRAMMAR.ASSETS, GRAMMAR.ManifestError, GRAMMAR.parse

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

def verify(path: Path) -> dict:
    try: mode, assets, inputs = parse(path)
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
            "failure_code": "none", "detail": "none", "assets": public, **inputs}

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
