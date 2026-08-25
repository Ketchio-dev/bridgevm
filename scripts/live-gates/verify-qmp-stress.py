#!/usr/bin/env python3
"""Fail-closed verifier for a retained t10 QMP full-workspace stress result."""
import argparse, gzip, hashlib, re, tempfile
from pathlib import Path

ROUND = re.compile(r"round=(\d+) log_sha256=([0-9a-f]{64})$")

def verify(out: Path) -> None:
    baseline = dict(line.split("=", 1) for line in (out / "baseline.txt").read_text().splitlines())
    if baseline != {"baseline_iterations": "20", "baseline_einval_then_econnrefused": "20"}:
        raise ValueError("baseline did not reproduce EINVAL then ECONNREFUSED 20/20")
    lines = (out / "summary.txt").read_text().splitlines()
    rounds = [ROUND.fullmatch(line) for line in lines if line.startswith("round=")]
    if len(rounds) != 60 or any(match is None for match in rounds):
        raise ValueError("summary does not contain 60 hashed rounds")
    if [int(match.group(1)) for match in rounds if match] != list(range(1, 61)):
        raise ValueError("round numbering is not exactly 1 through 60")
    fields = dict(line.split("=", 1) for line in lines if not line.startswith("round="))
    required = {"baseline_iterations":"20", "baseline_einval_then_econnrefused":"20",
        "workspace_rounds_required":"60", "workspace_rounds_passed":"60",
        "load_processes":"24", "command":"cargo test --workspace --locked"}
    if fields != required:
        raise ValueError("summary contract mismatch")
    logs = sorted((out / "rounds").glob("round-*.log.gz"))
    if len(logs) != 60:
        raise ValueError("retained compressed round-log count is not 60")
    for match, log in zip(rounds, logs):
        with gzip.open(log, "rb") as stream: digest = hashlib.sha256(stream.read()).hexdigest()
        if digest != match.group(2): raise ValueError(f"round log hash mismatch: {log.name}")

def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp); (out/"rounds").mkdir()
        (out/"baseline.txt").write_text("baseline_iterations=20\nbaseline_einval_then_econnrefused=20\n")
        digest=hashlib.sha256(b"x").hexdigest()
        hashes="".join(f"round={n} log_sha256={digest}\n" for n in range(1,61))
        tail="baseline_iterations=20\nbaseline_einval_then_econnrefused=20\nworkspace_rounds_required=60\nworkspace_rounds_passed=60\nload_processes=24\ncommand=cargo test --workspace --locked\n"
        (out/"summary.txt").write_text(hashes+tail)
        for n in range(1,61):
            with gzip.open(out/"rounds"/f"round-{n:02}.log.gz", "wb") as stream: stream.write(b"x")
        verify(out)
        for old,new in [("workspace_rounds_passed=60","workspace_rounds_passed=59"),
                        ("baseline_einval_then_econnrefused=20","baseline_einval_then_econnrefused=19")]:
            target=out/("baseline.txt" if old.startswith("baseline_") else "summary.txt")
            saved=target.read_text(); target.write_text(saved.replace(old,new,1))
            try: verify(out)
            except ValueError: pass
            else: raise AssertionError(f"mutation survived: {old}")
            target.write_text(saved)
        changed=out/"rounds/round-60.log.gz"
        with gzip.open(changed,"wb") as stream: stream.write(b"changed")
        try: verify(out)
        except ValueError: pass
        else: raise AssertionError("changed retained log survived its SHA-256")
    print("PASS: QMP stress receipt verifier and mutations")

def main() -> None:
    p=argparse.ArgumentParser(); p.add_argument("--out",type=Path); p.add_argument("--self-test",action="store_true")
    a=p.parse_args(); self_test() if a.self_test else verify(a.out)
if __name__ == "__main__": main()
