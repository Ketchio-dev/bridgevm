#!/usr/bin/env python3
"""Validate one retained 20-AppX diagnostic observation."""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import math
import os
import re
import stat
import tempfile
from pathlib import Path

RESULT = ("id", "identity_verified", "launch", "visible", "clean_shutdown", "samples", "series_sha256", "loaded_modules", "reason")
HEX = re.compile(r"[0-9a-f]{64}")
MODULES = {"d3d11.dll", "d3d12.dll", "dxgi.dll", "opengl32.dll", "vulkan-1.dll", "vulkan_virtio.dll"}
REASON = re.compile(r"[a-z0-9-]{2,64}")


class Refusal(ValueError):
    pass


def bounded(path: Path, limit: int) -> bytes:
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise Refusal(f"unsafe evidence file: {path.name}")
        raw = os.read(fd, limit + 1)
    finally:
        os.close(fd)
    if len(raw) > limit:
        raise Refusal(f"oversized evidence file: {path.name}")
    return raw


def tsv(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    raw = bounded(path, 1_000_000)
    if not raw.endswith(b"\n") or b"\r" in raw or b"\0" in raw:
        raise Refusal(f"invalid TSV framing: {path.name}")
    reader = csv.DictReader(io.StringIO(raw.decode("utf-8")), delimiter="\t")
    return list(reader.fieldnames or ()), list(reader)


def read_series(path: Path) -> tuple[bytes, list[float]]:
    raw = bounded(path, 8_000_000)
    try:
        values = [float(line) for line in raw.splitlines()]
    except ValueError as error:
        raise Refusal("invalid frame-time number") from error
    if not values or any(not math.isfinite(value) or value <= 0 or value > 60_000 for value in values):
        raise Refusal("invalid frame-time series")
    return raw, values


def validate(candidates: Path, results: Path, evidence: Path) -> dict[str, object]:
    info = evidence.lstat()
    if not stat.S_ISDIR(info.st_mode) or stat.S_ISLNK(info.st_mode):
        raise Refusal("unsafe evidence directory")
    candidate_fields, candidate_rows = tsv(candidates)
    result_fields, result_rows = tsv(results)
    if candidate_fields[:1] != ["id"] or result_fields != list(RESULT):
        raise Refusal("observation columns differ from the fixed schema")
    ids = [row["id"] for row in candidate_rows]
    if len(ids) != 20 or len(set(ids)) != 20 or [row["id"] for row in result_rows] != ids:
        raise Refusal("observation must preserve exactly 20 unique candidate rows")
    launched = visible = clean = with_series = 0
    api_rows = {"vulkan": 0, "d3d11": 0, "d3d12": 0, "opengl": 0}
    for row in result_rows:
        if row["identity_verified"] != "yes":
            raise Refusal(f"guest identity was not verified for {row['id']}")
        if any(row[key] not in {"yes", "no"} for key in ("launch", "visible", "clean_shutdown")):
            raise Refusal("observation booleans are invalid")
        if not REASON.fullmatch(row["reason"]):
            raise Refusal("observation reason is invalid")
        try:
            samples = int(row["samples"])
        except ValueError as error:
            raise Refusal("sample count is invalid") from error
        if samples < 0:
            raise Refusal("sample count is negative")
        series_path = evidence / f"{row['id']}.frametimes-ms"
        if samples:
            raw, values = read_series(series_path)
            if len(values) != samples or not HEX.fullmatch(row["series_sha256"]) or hashlib.sha256(raw).hexdigest() != row["series_sha256"]:
                raise Refusal("frame-time sample/hash mismatch")
            with_series += 1
        elif row["series_sha256"] != "absent" or series_path.exists() or series_path.is_symlink():
            raise Refusal("zero-sample row has fabricated or stray series evidence")
        modules = set() if row["loaded_modules"] == "none" else set(row["loaded_modules"].split(","))
        if any(module not in MODULES for module in modules) or row["loaded_modules"] != (",".join(sorted(modules)) if modules else "none"):
            raise Refusal("loaded-module evidence is invalid or unsorted")
        launched += row["launch"] == "yes"; visible += row["visible"] == "yes"; clean += row["clean_shutdown"] == "yes"
        api_rows["vulkan"] += bool(modules & {"vulkan-1.dll", "vulkan_virtio.dll"})
        api_rows["d3d12"] += "d3d12.dll" in modules
        api_rows["d3d11"] += bool(modules & {"d3d11.dll", "dxgi.dll"})
        api_rows["opengl"] += "opengl32.dll" in modules
    rendered = bounded(results, 1_000_000)
    return {
        "rows_observed": 20, "identities_verified": 20, "apps_launched": launched,
        "apps_visible": visible, "clean_shutdowns": clean, "series_rows": with_series,
        "vulkan_module_rows": api_rows["vulkan"], "d3d11_module_rows": api_rows["d3d11"],
        "d3d12_module_rows": api_rows["d3d12"], "opengl_module_rows": api_rows["opengl"],
        "observation_sha256": hashlib.sha256(rendered).hexdigest(),
    }


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary); evidence = root / "evidence"; evidence.mkdir()
        candidates = root / "candidates.tsv"; results = root / "results.tsv"
        candidates.write_text("id\tpackage\n" + "".join(f"real-app-{n:02d}\tpkg{n}\n" for n in range(20)))
        lines = ["\t".join(RESULT)]
        for number in range(20):
            ident = f"real-app-{number:02d}"; raw = b"16.667\n17\n"
            (evidence / f"{ident}.frametimes-ms").write_bytes(raw)
            lines.append(f"{ident}\tyes\tyes\tyes\tyes\t2\t{hashlib.sha256(raw).hexdigest()}\td3d11.dll,dxgi.dll\tcompleted")
        results.write_text("\n".join(lines) + "\n")
        assert validate(candidates, results, evidence)["rows_observed"] == 20
        saved = results.read_text(); results.write_text(saved.replace("\t2\t", "\t3\t", 1))
        try: validate(candidates, results, evidence)
        except Refusal: pass
        else: raise AssertionError("fabricated sample count survived")
        results.write_text(saved)
        target = evidence / "real-app-19.frametimes-ms"; target.unlink(); target.symlink_to(evidence / "real-app-18.frametimes-ms")
        try: validate(candidates, results, evidence)
        except OSError: pass
        else: raise AssertionError("series symlink survived")
    print("PASS: compatibility diagnostic observation and mutations")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidates", type=Path)
    parser.add_argument("--results", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test: self_test(); return 0
        summary = validate(args.candidates, args.results, args.evidence)
        rendered = json.dumps(summary, sort_keys=True, indent=2) + "\n"
        if args.json_out: args.json_out.write_text(rendered)
        else: print(rendered, end="")
    except (OSError, UnicodeError, Refusal, ValueError) as error:
        print(f"REFUSED: {error}", file=os.sys.stderr); return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
