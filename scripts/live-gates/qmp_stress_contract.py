#!/usr/bin/env python3
"""Strict hosted-QMP stress evidence writer, verifier, and bounded round runner."""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import json
import os
import pathlib
import platform
import re
import shutil
import stat
import subprocess
import tempfile
SCHEMA = "bridgevm.qmp-stress-hosted-receipt.v1"
ROUNDS_SCHEMA = "bridgevm.qmp-stress-rounds.v1"
COMMAND = ["cargo", "test", "--workspace", "--locked"]
COMMAND_TEXT = "cargo test --workspace --locked"
ROUND_COUNT = 60
BASELINE_COUNT = 20
LOAD_COUNT = 24
ROUND_HEADER = "schema\tround\tlog\traw_sha256\tgzip_sha256\traw_bytes\tgzip_bytes"
SHA = re.compile(r"[0-9a-f]{64}")
COMMIT = re.compile(r"[0-9a-f]{40}")
ROUND_ROW = re.compile(r"([^\t]+)\t([1-9][0-9]*)\t([^\t]+)\t([^\t]+)\t([^\t]+)\t(0|[1-9][0-9]*)\t(0|[1-9][0-9]*)")
RECEIPT_KEYS = {
    "schema_version", "venue", "workflow", "job", "repository", "commit", "workflow_head_sha",
    "run_id", "run_attempt", "runner_os", "runner_arch", "runner_image",
    "macos_version", "rustc_version", "command", "load_processes",
    "baseline_iterations", "baseline_matches", "workspace_rounds_required",
    "workspace_rounds_passed", "sample_count", "passes", "failures",
    "outcome", "pass", "failure_stage", "baseline_sha256",
    "rounds_sha256", "summary_sha256", "failed_log_name",
    "failed_log_sha256", "started_at", "completed_at",
}


class ContractError(ValueError):
    pass

def fail(message: str) -> None:
    raise ContractError(message)

def artifact_root(path: pathlib.Path) -> pathlib.Path:
    if not path.is_absolute() or ".." in path.parts:
        fail("output path must be absolute and traversal-free")
    return path

def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()

def regular(path: pathlib.Path, maximum: int = 128 * 1024 * 1024) -> int:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        fail(f"missing file: {path.name}")
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        fail(f"not a regular non-symlink file: {path.name}")
    if metadata.st_size > maximum:
        fail(f"file exceeds size limit: {path.name}")
    return metadata.st_size

def strict_object(path: pathlib.Path) -> dict[str, object]:
    regular(path, 64 * 1024)

    def unique(pairs: list[tuple[str, object]]) -> dict[str, object]:
        result: dict[str, object] = {}
        for key, value in pairs:
            if key in result:
                fail(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    try:
        result = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"invalid receipt JSON: {error}")
    if not isinstance(result, dict):
        fail("receipt must be a JSON object")
    return result

def canonical_int(value: object, name: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        fail(f"{name} must be an integer >= {minimum}")
    return value

def canonical_string(value: object, name: str) -> str:
    if not isinstance(value, str) or not value or "\n" in value or "\r" in value:
        fail(f"{name} must be a non-empty single-line string")
    return value

def parse_time(value: object, name: str) -> dt.datetime:
    text = canonical_string(value, name)
    if re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", text) is None:
        fail(f"{name} must be canonical whole-second UTC")
    try:
        parsed = dt.datetime.fromisoformat(text[:-1] + "+00:00")
    except ValueError:
        fail(f"{name} is not an ISO-8601 timestamp")
    return parsed

def baseline(path: pathlib.Path) -> tuple[int, int]:
    regular(path, 1024)
    lines = path.read_text(encoding="utf-8").splitlines()
    if lines != [
        "baseline_iterations=20",
        "baseline_einval_then_econnrefused=20",
    ]:
        fail("baseline did not reproduce EINVAL then ECONNREFUSED 20/20")
    return BASELINE_COUNT, BASELINE_COUNT

def round_rows(out: pathlib.Path) -> list[dict[str, object]]:
    path = out / "rounds.tsv"
    regular(path, 64 * 1024)
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != ROUND_HEADER:
        fail("round manifest header mismatch")
    rows: list[dict[str, object]] = []
    for expected, line in enumerate(lines[1:], 1):
        match = ROUND_ROW.fullmatch(line)
        if match is None:
            fail(f"invalid round manifest row {expected}")
        schema, number_text, name, raw_hash, gzip_hash, raw_size, gzip_size = match.groups()
        number = int(number_text)
        expected_name = f"round-{expected:02}.log.gz"
        if schema != ROUNDS_SCHEMA or number != expected or name != expected_name:
            fail(f"round manifest sequence mismatch at row {expected}")
        if SHA.fullmatch(raw_hash) is None or SHA.fullmatch(gzip_hash) is None:
            fail(f"invalid round hash at row {expected}")
        rows.append({"number": number, "name": name, "raw_hash": raw_hash,
                     "gzip_hash": gzip_hash, "raw_size": int(raw_size),
                     "gzip_size": int(gzip_size)})
    if len(rows) > ROUND_COUNT:
        fail("round manifest exceeds 60 rows")
    return rows

def inspect_gzip(path: pathlib.Path) -> tuple[str, int]:
    regular(path)
    digest = hashlib.sha256()
    total = 0
    try:
        with gzip.open(path, "rb") as stream:
            while block := stream.read(1024 * 1024):
                total += len(block)
                if total > 128 * 1024 * 1024:
                    fail(f"expanded log exceeds size limit: {path.name}")
                digest.update(block)
    except (gzip.BadGzipFile, EOFError, OSError) as error:
        fail(f"invalid gzip log {path.name}: {error}")
    return digest.hexdigest(), total

def summary_text(rows: list[dict[str, object]], baseline_ok: bool) -> str:
    lines = [f"round={row['number']} log_sha256={row['raw_hash']}" for row in rows]
    lines.extend([
        f"baseline_iterations={BASELINE_COUNT if baseline_ok else 0}",
        f"baseline_einval_then_econnrefused={BASELINE_COUNT if baseline_ok else 0}",
        f"workspace_rounds_required={ROUND_COUNT}",
        f"workspace_rounds_passed={len(rows)}",
        f"load_processes={LOAD_COUNT}",
        f"command={COMMAND_TEXT}",
    ])
    return "\n".join(lines) + "\n"

def inspect_artifacts(out: pathlib.Path, require_success: bool) -> tuple[list[dict[str, object]], bool]:
    if out.is_symlink() or not out.is_dir():
        fail("output must be a non-symlink directory")
    rounds_dir = out / "rounds"
    if rounds_dir.is_symlink() or not rounds_dir.is_dir():
        fail("rounds must be a non-symlink directory")
    rows = round_rows(out)
    baseline_ok = False
    if (out / "baseline.txt").exists():
        baseline(out / "baseline.txt")
        baseline_ok = True
    for row in rows:
        path = rounds_dir / str(row["name"])
        raw_hash, raw_size = inspect_gzip(path)
        if raw_hash != row["raw_hash"] or raw_size != row["raw_size"]:
            fail(f"raw round evidence mismatch: {path.name}")
        if sha256(path) != row["gzip_hash"] or regular(path) != row["gzip_size"]:
            fail(f"compressed round evidence mismatch: {path.name}")
    regular(out / "summary.txt", 128 * 1024)
    if (out / "summary.txt").read_text(encoding="utf-8") != summary_text(rows, baseline_ok):
        fail("summary does not exactly match retained evidence")
    if require_success and (not baseline_ok or len(rows) != ROUND_COUNT):
        fail("successful evidence requires baseline 20/20 and exactly 60 rounds")
    return rows, baseline_ok

def expected_inventory(out: pathlib.Path, receipt: dict[str, object], rows: list[dict[str, object]]) -> None:
    root = {"rounds", "rounds.tsv", "summary.txt", "receipt.json"}
    if receipt["baseline_sha256"] is not None:
        root.add("baseline.txt")
    failed_name = receipt["failed_log_name"]
    if failed_name is not None:
        root.add(str(failed_name))
    if {path.name for path in out.iterdir()} != root:
        fail("output root contains missing or unexpected entries")
    expected_rounds = {str(row["name"]) for row in rows}
    if {path.name for path in (out / "rounds").iterdir()} != expected_rounds:
        fail("rounds directory contains missing or unexpected entries")


def verify(out: pathlib.Path, allow_failure: bool, expected: argparse.Namespace | None = None) -> None:
    receipt = strict_object(out / "receipt.json")
    if set(receipt) != RECEIPT_KEYS:
        fail(f"receipt keys mismatch: {sorted(set(receipt) ^ RECEIPT_KEYS)}")
    fixed = {"schema_version": SCHEMA, "venue": "github-hosted-macos",
             "workflow": ".github/workflows/qmp-stress.yml", "job": "qmp-stress",
             "command": COMMAND_TEXT, "load_processes": LOAD_COUNT,
             "workspace_rounds_required": ROUND_COUNT}
    for key, value in fixed.items():
        if receipt[key] != value:
            fail(f"receipt {key} mismatch")
    repository = canonical_string(receipt["repository"], "repository")
    if re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) is None:
        fail("repository must be an owner/name pair")
    commit = canonical_string(receipt["commit"], "commit")
    if COMMIT.fullmatch(commit) is None:
        fail("commit must be a lowercase 40-hex SHA")
    workflow_head_sha = canonical_string(receipt["workflow_head_sha"], "workflow_head_sha")
    if workflow_head_sha != commit:
        fail("workflow head SHA must equal the tested commit")
    run_id = canonical_int(receipt["run_id"], "run_id", 1)
    run_attempt = canonical_int(receipt["run_attempt"], "run_attempt", 1)
    for name in ("runner_os", "runner_arch", "runner_image", "macos_version", "rustc_version"):
        canonical_string(receipt[name], name)
    if receipt["runner_os"] != "macOS":
        fail("runner_os must be macOS")
    if receipt["runner_arch"] not in {"ARM64", "X64"} or re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", receipt["macos_version"]) is None or not receipt["rustc_version"].startswith("rustc 1.97.0 "):
        fail("hosted runner architecture, macOS version, or pinned rustc is invalid")
    started = parse_time(receipt["started_at"], "started_at")
    completed = parse_time(receipt["completed_at"], "completed_at")
    if completed < started:
        fail("receipt timestamps are reversed")
    passed = receipt["pass"]
    if not isinstance(passed, bool):
        fail("pass must be a boolean")
    if not passed and not allow_failure:
        fail("receipt records a failed hosted campaign")
    rows, baseline_ok = inspect_artifacts(out, passed)
    completed_rounds = len(rows)
    for name in ("workspace_rounds_passed", "sample_count", "passes"):
        if canonical_int(receipt[name], name) != completed_rounds:
            fail(f"receipt {name} disagrees with round evidence")
    evidence_complete = baseline_ok and completed_rounds == ROUND_COUNT
    if passed and not evidence_complete:
        fail("passing receipt lacks complete evidence")
    if receipt["outcome"] != ("completed" if passed else "failed"):
        fail("receipt outcome/pass disagrees with evidence")
    if canonical_int(receipt["failures"], "failures") != (0 if passed else 1):
        fail("receipt failure count mismatch")
    if receipt["baseline_iterations"] != (BASELINE_COUNT if baseline_ok else 0):
        fail("baseline iteration count mismatch")
    if receipt["baseline_matches"] != (BASELINE_COUNT if baseline_ok else 0):
        fail("baseline match count mismatch")
    failure_stage = canonical_string(receipt["failure_stage"], "failure_stage")
    if passed != (failure_stage == "none"):
        fail("failure_stage/pass mismatch")
    baseline_hash = sha256(out / "baseline.txt") if baseline_ok else None
    if receipt["baseline_sha256"] != baseline_hash:
        fail("baseline hash mismatch")
    if receipt["rounds_sha256"] != sha256(out / "rounds.tsv"):
        fail("round manifest hash mismatch")
    if receipt["summary_sha256"] != sha256(out / "summary.txt"):
        fail("summary hash mismatch")
    failed_name = receipt["failed_log_name"]
    failed_hash = receipt["failed_log_sha256"]
    if failed_name is None or failed_hash is None:
        if failed_name is not None or failed_hash is not None:
            fail("failed log name and hash must both be null or strings")
    else:
        name = canonical_string(failed_name, "failed_log_name")
        if re.fullmatch(r"failed-round-(?:0[1-9]|[1-5][0-9]|60)\.log\.gz", name) is None:
            fail("failed log name is not canonical")
        if not isinstance(failed_hash, str) or SHA.fullmatch(failed_hash) is None:
            fail("failed log hash is invalid")
        inspect_gzip(out / name)
        if sha256(out / name) != failed_hash:
            fail("failed log hash mismatch")
        expected_name = f"failed-round-{completed_rounds + 1:02}.log.gz"
        if name != expected_name or failure_stage != f"round-{completed_rounds + 1:02}":
            fail("failed log does not identify the next attempted round")
    if failure_stage.startswith("round-") and failed_name is None:
        fail("round failure receipt must retain its failed log")
    if not passed and not (failure_stage in {"load", "baseline-build", "baseline", "campaign-deadline", "source-mutation"}
                           or re.fullmatch(r"round-(?:0[1-9]|[1-5][0-9]|60)", failure_stage)):
        fail("failure stage is not recognized")
    if passed and failed_name is not None:
        fail("successful receipt cannot retain a failed log")
    expected_inventory(out, receipt, rows)
    if expected is not None:
        for name, actual in (("repository", repository), ("commit", commit),
                             ("workflow_head_sha", workflow_head_sha),
                             ("run_id", str(run_id)), ("run_attempt", str(run_attempt))):
            wanted = getattr(expected, f"expected_{name}", None)
            if wanted is not None and str(actual) != wanted:
                fail(f"receipt {name} does not match expected value")


def atomic_text(path: pathlib.Path, text: str) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(text, encoding="utf-8")
    os.replace(temporary, path)


def write_receipt(args: argparse.Namespace) -> None:
    out = artifact_root(args.out)
    rows = round_rows(out)
    baseline_ok = False
    if (out / "baseline.txt").exists():
        baseline(out / "baseline.txt")
        baseline_ok = True
    atomic_text(out / "summary.txt", summary_text(rows, baseline_ok))
    failed = sorted(out.glob("failed-round-*.log.gz"))
    if len(failed) > 1:
        fail("more than one failed round log exists")
    success = args.outcome == "completed"
    if success and (not baseline_ok or len(rows) != ROUND_COUNT or failed):
        fail("cannot write a successful receipt for incomplete evidence")
    if not success and args.failure_stage == "none":
        fail("failed receipt needs a failure stage")
    receipt = {
        "schema_version": SCHEMA, "venue": "github-hosted-macos",
        "workflow": ".github/workflows/qmp-stress.yml", "job": "qmp-stress",
        "repository": args.repository, "commit": args.commit,
        "workflow_head_sha": args.workflow_head_sha,
        "run_id": int(args.run_id), "run_attempt": int(args.run_attempt),
        "runner_os": os.environ.get("RUNNER_OS", ""),
        "runner_arch": os.environ.get("RUNNER_ARCH", ""),
        "runner_image": os.environ.get("ImageOS", "") or platform.platform(),
        "macos_version": platform.mac_ver()[0],
        "rustc_version": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "command": COMMAND_TEXT, "load_processes": LOAD_COUNT,
        "baseline_iterations": BASELINE_COUNT if baseline_ok else 0,
        "baseline_matches": BASELINE_COUNT if baseline_ok else 0,
        "workspace_rounds_required": ROUND_COUNT,
        "workspace_rounds_passed": len(rows), "sample_count": len(rows),
        "passes": len(rows), "failures": 0 if success else 1,
        "outcome": args.outcome, "pass": success,
        "failure_stage": args.failure_stage,
        "baseline_sha256": sha256(out / "baseline.txt") if baseline_ok else None,
        "rounds_sha256": sha256(out / "rounds.tsv"),
        "summary_sha256": sha256(out / "summary.txt"),
        "failed_log_name": failed[0].name if failed else None,
        "failed_log_sha256": sha256(failed[0]) if failed else None,
        "started_at": args.started_at,
        "completed_at": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    atomic_text(out / "receipt.json", json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    verify(out, allow_failure=not success)


def run_round(args: argparse.Namespace) -> None:
    out = artifact_root(args.out)
    rows = round_rows(out)
    if args.round != len(rows) + 1 or not 1 <= args.round <= ROUND_COUNT:
        fail("round must be the next sequential number in 1..60")
    raw = out / f"round-{args.round:02}.log"
    final = out / "rounds" / f"round-{args.round:02}.log.gz"
    failed = out / f"failed-round-{args.round:02}.log.gz"
    if raw.exists() or final.exists() or failed.exists():
        fail("round output already exists")
    status = 1
    with raw.open("xb") as stream:
        try:
            status = subprocess.run(COMMAND, cwd=args.repo, stdout=stream,
                                    stderr=subprocess.STDOUT, timeout=args.timeout).returncode
        except subprocess.TimeoutExpired:
            stream.write(b"\nbridgevm: round timed out\n")
            status = 124
    destination = final if status == 0 else failed
    with raw.open("rb") as source, destination.open("xb") as target:
        with gzip.GzipFile(filename="", mode="wb", fileobj=target, compresslevel=9, mtime=0) as archive:
            shutil.copyfileobj(source, archive)
    raw_hash = sha256(raw)
    raw_size = regular(raw)
    raw.unlink()
    if status != 0:
        raise SystemExit(status)
    row = "\t".join([ROUNDS_SCHEMA, str(args.round), final.name, raw_hash,
                     sha256(final), str(raw_size), str(regular(final))]) + "\n"
    with (out / "rounds.tsv").open("a", encoding="utf-8", newline="") as stream:
        stream.write(row)


def self_test() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        out = pathlib.Path(temporary)
        (out / "rounds").mkdir()
        (out / "baseline.txt").write_text(
            "baseline_iterations=20\nbaseline_einval_then_econnrefused=20\n", encoding="utf-8")
        rows = [ROUND_HEADER]
        for number in range(1, ROUND_COUNT + 1):
            raw = f"round {number}\n".encode()
            path = out / "rounds" / f"round-{number:02}.log.gz"
            with path.open("wb") as target:
                with gzip.GzipFile(filename="", mode="wb", fileobj=target, mtime=0) as archive:
                    archive.write(raw)
            rows.append("\t".join([ROUNDS_SCHEMA, str(number), path.name,
                        hashlib.sha256(raw).hexdigest(), sha256(path), str(len(raw)), str(path.stat().st_size)]))
        (out / "rounds.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8")
        namespace = argparse.Namespace(out=out, outcome="completed", failure_stage="none",
            repository="owner/repo", commit="a" * 40, workflow_head_sha="a" * 40,
            run_id="7", run_attempt="1",
            started_at="2000-01-01T00:00:00Z")
        environment = {"RUNNER_OS": "macOS", "RUNNER_ARCH": "ARM64", "ImageOS": "macos15"}
        saved = {key: os.environ.get(key) for key in environment}
        saved_mac_ver = platform.mac_ver
        saved_check_output = subprocess.check_output
        platform.mac_ver = lambda: ("15.6.1", ("", "", ""), "")
        subprocess.check_output = lambda *args, **kwargs: "rustc 1.97.0 (test 2000-01-01)\n"
        os.environ.update(environment)
        try:
            write_receipt(namespace)
            verify(out, False)
            original = (out / "receipt.json").read_text()
            (out / "receipt.json").write_text(original.replace('"venue":', '"venue":"duplicate",\n  "venue":', 1))
            try:
                verify(out, False)
            except ContractError:
                pass
            else:
                raise AssertionError("duplicate receipt key survived")
            (out / "receipt.json").write_text(original)
            baseline_path = out / "baseline.txt"; baseline_original = baseline_path.read_text()
            baseline_path.write_text(baseline_original.replace("=20\n", "=19\n", 1))
            try: verify(out, False)
            except ContractError: pass
            else: raise AssertionError("19/20 baseline mutation survived")
            baseline_path.write_text(baseline_original)
            extra = out / "unexpected.txt"; extra.write_text("not evidence\n")
            try: verify(out, False)
            except ContractError: pass
            else: raise AssertionError("unexpected artifact survived")
            extra.unlink()
            changed = out / "rounds" / "round-60.log.gz"
            original_log = changed.read_bytes()
            changed.write_bytes(changed.read_bytes() + b"mutation")
            try:
                verify(out, False)
            except ContractError:
                pass
            else:
                raise AssertionError("mutated compressed log survived")
            changed.write_bytes(original_log)
            manifest = (out / "rounds.tsv").read_text().splitlines()
            (out / "rounds.tsv").write_text("\n".join(manifest[:-1]) + "\n")
            changed.rename(out / "failed-round-60.log.gz")
            namespace.outcome = "failed"
            namespace.failure_stage = "round-60"
            write_receipt(namespace)
            verify(out, True)
            try:
                verify(out, False)
            except ContractError:
                pass
            else:
                raise AssertionError("failed receipt passed without --allow-failure")
        finally:
            platform.mac_ver = saved_mac_ver
            subprocess.check_output = saved_check_output
            for key, value in saved.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value
    print("PASS: hosted QMP stress contract, receipt, and adversarial mutations")
def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command_name", required=True)
    check = commands.add_parser("verify")
    check.add_argument("--out", type=pathlib.Path, required=True)
    check.add_argument("--allow-failure", action="store_true")
    for name in ("repository", "commit", "workflow-head-sha", "run-id", "run-attempt"):
        check.add_argument(f"--expected-{name}")
    write = commands.add_parser("write-receipt")
    write.add_argument("--out", type=pathlib.Path, required=True)
    write.add_argument("--outcome", choices=("completed", "failed"), required=True)
    write.add_argument("--failure-stage", required=True)
    write.add_argument("--repository", required=True)
    write.add_argument("--commit", required=True)
    write.add_argument("--workflow-head-sha", required=True)
    write.add_argument("--run-id", required=True)
    write.add_argument("--run-attempt", required=True)
    write.add_argument("--started-at", required=True)
    round_command = commands.add_parser("run-round")
    round_command.add_argument("--out", type=pathlib.Path, required=True)
    round_command.add_argument("--repo", type=pathlib.Path, required=True)
    round_command.add_argument("--round", type=int, required=True)
    round_command.add_argument("--timeout", type=int, default=900)
    commands.add_parser("self-test")
    return root


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command_name == "verify":
            verify(artifact_root(args.out), args.allow_failure, args)
        elif args.command_name == "write-receipt":
            write_receipt(args)
        elif args.command_name == "run-round":
            run_round(args)
        else:
            self_test()
    except (ContractError, OSError, UnicodeError, subprocess.SubprocessError) as error:
        print(f"QMP stress contract: FAIL ({error})", file=os.sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
