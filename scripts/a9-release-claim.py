#!/usr/bin/env python3
"""Build and strictly verify the combined A9 release-claim receipt.

A9 needs ISO installation (T17) and installed-disk import (T19) to each complete a
retained clean-machine app journey in the supported 3D-off configuration. Neither
journey receipt can say that alone: the live writers cannot observe a clean machine
or hosted CI, and the T19 verifier never grants A9 by itself. This receipt binds, by
SHA-256 of their exact bytes:

- one T17 and one T19 receipt for one commit, each accepted by its existing verifier
  with --expected-commit (the strict read the live tiers perform);
- the Hosted full project check and Security and quality attestations written for
  that commit by scripts/fetch-a9-ci-attestation.py;
- an operator clean-host attestation (schemas/a9-clean-host-attestation-v1.json) whose
  SHA-256 the operator recorded beforehand, as B8 binds its attestation.

Any contradiction refuses with exit status 1: an input an existing verifier rejects; a
commit, host, app artifact, workflow or CI reference that differs; an unbound or late
clean-host attestation. Consistent inputs produce a receipt whose fixed rule is

    claim_eligible = (unmet_conditions == [])

A condition is unmet when a journey is not a release campaign, is not 3/3 passing, or
is not signed developer-id-notarized; when either CI run did not conclude success; or
when no clean-host attestation is bound. 3D injection is refused outright. The tool
never edits the capability registry; criterion_pass and capability_promotion stay false.

    scripts/a9-release-claim.py build --commit SHA --t17 T17.json --t19 T19.json \\
        --hosted-full-ci HOSTED.json --security-quality-ci SECURITY.json \\
        [--clean-host-attestation FILE --clean-host-attestation-sha256 SHA] --out CLAIM.json
    scripts/a9-release-claim.py verify CLAIM.json --t17 ... --security-quality-ci ... \\
        [--clean-host-attestation FILE] [--require-claim-eligible]
    scripts/a9-release-claim.py --self-test
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import a9_ci_attestation as CI  # noqa: E402


def _load(name: str, relative: str):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {relative}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


T17 = _load("a9_claim_t17_verifier", "scripts/verify-windows-product-e2e-receipt.py")
T19 = _load("a9_claim_t19_verifier", "scripts/verify-windows-import-product-e2e-receipt.py")
JOURNEYS = {"t17": T17, "t19": T19}
CI_SLOTS = {"hosted_full": ("hosted-full", "hosted_ci_run_id"),
            "security_quality": ("security-quality", "security_ci_run_id")}
SCHEMA = "bridgevm.a9-release-claim.v1"
CLEAN_HOST_SCHEMA = "bridgevm.a9-clean-host-attestation.v1"
CONDITIONS = tuple(f"{name}-{tail}" for name in JOURNEYS for tail in (
    "campaign-not-release", "not-three-of-three-passing", "signing-not-developer-id-notarized")) + (
    "hosted-full-ci-not-success", "security-quality-ci-not-success", "clean-host-attestation-absent")
FIELDS = frozenset({"schema_version", "criterion", "commit", "three_d_injection", "host_model",
                    "macos_version", "app_artifact_sha256", "app_executable_sha256", "journeys", "ci",
                    "clean_host", "clean_machine", "unmet_conditions", "claim_eligible",
                    "criterion_pass", "capability_promotion"})
JOURNEY_FIELDS = frozenset({"receipt_sha256", "job_id", "campaign_mode", "expected_runs", "run_count",
                            "passes", "pass", "artifact_signing_class", "started_at", "finished_at"})
CI_FIELDS = frozenset({"attestation_sha256", "run_id", "conclusion"})
CLEAN_FIELDS = frozenset({"attestation_sha256", "attested_at"})
CLEAN_HOST_FIELDS = frozenset({"schema_version", "criterion", "commit", "host_model", "macos_version",
                               "attested_at", "attested_by", "clean_machine_definition"})
SHARED = ("host_model", "macos_version", "app_artifact_sha256", "app_executable_sha256")
SIGNING = ("unverified", "development-ad-hoc", "development-signed", "developer-id-notarized")
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
TEXT = re.compile(r"[^/\\\x00-\x1f\x7f]{1,128}\Z")
DEFINITION = re.compile(r"[^\x00-\x09\x0b-\x1f\x7f]{1,4096}\Z")
MAX_INPUT = 1024 * 1024
VERIFIER_ERRORS = (ValueError, TypeError, KeyError, AttributeError)


class ClaimError(ValueError):
    """The A9 claim inputs or receipt are missing, malformed or inconsistent."""


def parse(data: bytes, label: str) -> object:
    def unique(pairs: list[tuple[str, object]]) -> dict:
        value: dict = {}
        for key, item in pairs:
            if key in value:
                raise ClaimError(f"{label}: duplicate field {key!r}")
            value[key] = item
        return value

    def constant(name: str) -> object:
        raise ClaimError(f"{label}: non-finite number {name}")
    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=unique, parse_constant=constant)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ClaimError(f"{label}: malformed JSON ({error})") from error


def read_input(path: Path, label: str) -> tuple[bytes, object]:
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK)
    except OSError as error:
        raise ClaimError(f"{label}: cannot open {path} as a regular non-symlink file ({error.strerror})") from error
    chunks: list[bytes] = []
    try:
        info = os.fstat(descriptor)
        if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= MAX_INPUT:
            raise ClaimError(f"{label}: {path} is not a regular file of 1..{MAX_INPUT} bytes")
        while chunk := os.read(descriptor, 65536):
            chunks.append(chunk)
            if sum(map(len, chunks)) > MAX_INPUT:
                raise ClaimError(f"{label}: {path} grew past {MAX_INPUT} bytes while it was read")
    finally:
        os.close(descriptor)
    data = b"".join(chunks)
    return data, parse(data, label)


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _time(value: object, label: str) -> datetime:
    if type(value) is not str or not CI.TIMESTAMP.fullmatch(value):
        raise ClaimError(f"{label} must be a UTC second-resolution timestamp")
    try:
        return datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as error:
        raise ClaimError(f"{label} is not a real time") from error


def journey(name: str, data: bytes, value: object, commit: str) -> dict:
    label = f"{name.upper()} receipt"
    try:
        JOURNEYS[name].validate(value, expected_commit=commit)
    except VERIFIER_ERRORS as error:
        raise ClaimError(f"{label} refused by its existing verifier: {error}") from error
    if value["hosted_ci_commit"] != commit:
        raise ClaimError(f"{label} hosted_ci_commit {value['hosted_ci_commit']} is not {commit}")
    if value["three_d_injection"] is not False or value["criterion_pass"] or value["capability_promotion"]:
        raise ClaimError(f"{label} enables 3D injection or claims a criterion or promotion")
    record = {key: value[key] for key in JOURNEY_FIELDS - {"receipt_sha256"}}
    return {"receipt_sha256": _sha(data), **record}


def ci_run(slot: str, data: bytes, value: object, commit: str, receipts: dict) -> dict:
    kind, field = CI_SLOTS[slot]
    try:
        CI.validate(value, kind=kind, commit=commit)
    except CI.AttestationError as error:
        raise ClaimError(str(error)) from error
    for name, receipt in receipts.items():
        if receipt[field] not in ("absent", value["run_id"]):
            raise ClaimError(f"{name.upper()} receipt {field} {receipt[field]!r} is not attested run {value['run_id']}")
    return {"attestation_sha256": _sha(data), "run_id": value["run_id"], "conclusion": value["conclusion"]}


def clean_host(data: bytes, value: object, recorded_sha256: object, commit: str, receipts: dict) -> dict:
    if type(recorded_sha256) is not str or not SHA256.fullmatch(recorded_sha256):
        raise ClaimError("the recorded clean-host attestation SHA-256 must be 64 lowercase hex digits")
    if _sha(data) != recorded_sha256:
        raise ClaimError(f"clean-host attestation is unbound: its SHA-256 {_sha(data)} is not the recorded {recorded_sha256}")
    if not isinstance(value, dict) or frozenset(value) != CLEAN_HOST_FIELDS:
        raise ClaimError("clean-host attestation has missing or unknown fields")
    if value["schema_version"] != CLEAN_HOST_SCHEMA or value["criterion"] != "A9":
        raise ClaimError(f"clean-host attestation is not {CLEAN_HOST_SCHEMA} for A9")
    if value["commit"] != commit:
        raise ClaimError(f"clean-host attestation names commit {value['commit']!r}, not {commit}")
    for field in ("host_model", "macos_version"):
        if value[field] != receipts["t17"][field]:
            raise ClaimError(f"clean-host attestation {field} {value[field]!r} is not the journeys' {receipts['t17'][field]!r}")
    attested = _time(value["attested_at"], "clean-host attestation attested_at")
    if attested > min(_time(receipt["started_at"], "journey started_at") for receipt in receipts.values()):
        raise ClaimError("clean-host attestation postdates the start of a journey")
    if type(value["attested_by"]) is not str or not IDENTIFIER.fullmatch(value["attested_by"]):
        raise ClaimError("clean-host attestation attested_by must be an identifier")
    if type(value["clean_machine_definition"]) is not str or not DEFINITION.fullmatch(value["clean_machine_definition"]):
        raise ClaimError("clean-host attestation clean_machine_definition must be 1..4096 characters without control characters")
    return {"attestation_sha256": recorded_sha256, "attested_at": value["attested_at"]}


def unmet(receipt: dict) -> list[str]:
    result = []
    for name in JOURNEYS:
        item = receipt["journeys"][name]
        if item["campaign_mode"] != "release":
            result.append(f"{name}-campaign-not-release")
        if not (item["expected_runs"] == item["run_count"] == item["passes"] == 3 and item["pass"] is True):
            result.append(f"{name}-not-three-of-three-passing")
        if item["artifact_signing_class"] != "developer-id-notarized":
            result.append(f"{name}-signing-not-developer-id-notarized")
    for slot, (kind, _) in CI_SLOTS.items():
        if receipt["ci"][slot]["conclusion"] != "success":
            result.append(f"{kind}-ci-not-success")
    if receipt["clean_host"]["attestation_sha256"] == "absent":
        result.append("clean-host-attestation-absent")
    return result


def build(*, commit: str, t17: tuple, t19: tuple, hosted_full: tuple, security_quality: tuple,
          clean: tuple | None = None, clean_sha256: str | None = None) -> dict:
    """Each input is (exact bytes, parsed JSON); the result is a pure function of them."""
    if type(commit) is not str or not CI.COMMIT.fullmatch(commit):
        raise ClaimError("the claimed commit must be a full lowercase commit")
    receipts = {"t17": t17[1], "t19": t19[1]}
    journeys = {name: journey(name, *pair, commit) for name, pair in (("t17", t17), ("t19", t19))}
    for field in SHARED:
        first, second = receipts["t17"][field], receipts["t19"][field]
        if first == "absent" or first != second:
            raise ClaimError(f"T17 and T19 receipts do not share one {field}: {first!r} vs {second!r}")
    runs = {slot: ci_run(slot, *pair, commit, receipts)
            for slot, pair in (("hosted_full", hosted_full), ("security_quality", security_quality))}
    if (clean is None) != (clean_sha256 is None):
        raise ClaimError("a clean-host attestation and its recorded SHA-256 must be supplied together")
    host = ({"attestation_sha256": "absent", "attested_at": "absent"} if clean is None
            else clean_host(*clean, clean_sha256, commit, receipts))
    receipt = {"schema_version": SCHEMA, "criterion": "A9", "commit": commit, "three_d_injection": False,
               **{field: receipts["t17"][field] for field in SHARED}, "journeys": journeys, "ci": runs,
               "clean_host": host, "clean_machine": host["attestation_sha256"] != "absent",
               "criterion_pass": False, "capability_promotion": False}
    receipt["unmet_conditions"] = unmet(receipt)
    receipt["claim_eligible"] = not receipt["unmet_conditions"]
    return validate_receipt(receipt)


def _object(value: object, keys: frozenset, label: str) -> dict:
    if not isinstance(value, dict) or frozenset(value) != keys:
        raise ClaimError(f"{label} has missing or unknown fields")
    return value


def validate_receipt(receipt: object) -> dict:
    """Check the shape and the fixed rule of a combined receipt on its JSON alone."""
    receipt = _object(receipt, FIELDS, "A9 claim receipt")
    fixed = {"schema_version": SCHEMA, "criterion": "A9", "three_d_injection": False,
             "criterion_pass": False, "capability_promotion": False}
    for key, item in fixed.items():
        if type(receipt[key]) is not type(item) or receipt[key] != item:
            raise ClaimError(f"A9 claim receipt {key} must be {item!r}")
    if type(receipt["commit"]) is not str or not CI.COMMIT.fullmatch(receipt["commit"]):
        raise ClaimError("A9 claim receipt commit must be a full lowercase commit")
    for field in SHARED:
        pattern = TEXT if field in ("host_model", "macos_version") else SHA256
        if type(receipt[field]) is not str or not pattern.fullmatch(receipt[field]):
            raise ClaimError(f"A9 claim receipt {field} is malformed")
    starts = []
    for name, item in _object(receipt["journeys"], frozenset(JOURNEYS), "A9 claim journeys").items():
        item = _object(item, JOURNEY_FIELDS, f"A9 claim {name} journey")
        expected = {"pilot": 1, "release": 3}.get(item["campaign_mode"]) if type(item["campaign_mode"]) is str else None
        counts = [item[key] for key in ("expected_runs", "run_count", "passes")]
        if (expected is None or any(type(count) is not int for count in counts) or item["expected_runs"] != expected
                or not 0 <= item["passes"] <= item["run_count"] <= expected or type(item["pass"]) is not bool
                or item["artifact_signing_class"] not in SIGNING or type(item["job_id"]) is not str
                or not IDENTIFIER.fullmatch(item["job_id"]) or type(item["receipt_sha256"]) is not str
                or not SHA256.fullmatch(item["receipt_sha256"])):
            raise ClaimError(f"A9 claim {name} journey is malformed")
        starts.append(_time(item["started_at"], f"A9 claim {name} started_at"))
        if _time(item["finished_at"], f"A9 claim {name} finished_at") < starts[-1]:
            raise ClaimError(f"A9 claim {name} journey finished before it started")
    for slot, item in _object(receipt["ci"], frozenset(CI_SLOTS), "A9 claim CI").items():
        item = _object(item, CI_FIELDS, f"A9 claim {slot} CI")
        if (type(item["attestation_sha256"]) is not str or not SHA256.fullmatch(item["attestation_sha256"])
                or type(item["run_id"]) is not str or not CI.RUN_ID.fullmatch(item["run_id"])
                or type(item["conclusion"]) is not str or item["conclusion"] not in CI.CONCLUSIONS):
            raise ClaimError(f"A9 claim {slot} CI record is malformed")
    host = _object(receipt["clean_host"], CLEAN_FIELDS, "A9 claim clean_host")
    if host != {"attestation_sha256": "absent", "attested_at": "absent"}:
        if type(host["attestation_sha256"]) is not str or not SHA256.fullmatch(host["attestation_sha256"]):
            raise ClaimError("A9 claim clean_host attestation_sha256 is malformed")
        if _time(host["attested_at"], "A9 claim clean_host attested_at") > min(starts):
            raise ClaimError("A9 claim clean-host attestation postdates the start of a journey")
    if receipt["clean_machine"] is not (host["attestation_sha256"] != "absent"):
        raise ClaimError("A9 claim clean_machine must be true exactly when a clean-host attestation is bound")
    if receipt["unmet_conditions"] != unmet(receipt):
        raise ClaimError(f"A9 claim unmet_conditions must be {unmet(receipt)}")
    if receipt["claim_eligible"] is not (not receipt["unmet_conditions"]):
        raise ClaimError("A9 claim claim_eligible does not match the fixed release rule")
    return receipt


def first_difference(expected: object, actual: object, path: str = "receipt") -> str | None:
    if isinstance(expected, dict) and isinstance(actual, dict):
        for key in sorted(set(expected) | set(actual)):
            if key not in expected or key not in actual:
                return f"{path}.{key}"
            found = first_difference(expected[key], actual[key], f"{path}.{key}")
            if found:
                return found
        return None
    return path if type(expected) is not type(actual) or expected != actual else None


def verify(receipt: object, *, t17: tuple, t19: tuple, hosted_full: tuple, security_quality: tuple,
           clean: tuple | None = None, expected_commit: str | None = None) -> dict:
    receipt = validate_receipt(receipt)
    if expected_commit is not None and receipt["commit"] != expected_commit:
        raise ClaimError(f"A9 claim receipt commit {receipt['commit']} is not the expected {expected_commit}")
    bound = receipt["clean_host"]["attestation_sha256"]
    if (bound == "absent") != (clean is None):
        raise ClaimError("the clean-host attestation supplied does not match the receipt's binding"
                         if clean is not None else f"receipt binds clean-host attestation {bound}; supply it")
    rebuilt = build(commit=receipt["commit"], t17=t17, t19=t19, hosted_full=hosted_full,
                    security_quality=security_quality, clean=clean, clean_sha256=None if clean is None else bound)
    difference = first_difference(rebuilt, receipt)
    if difference:
        raise ClaimError(f"A9 claim {difference} does not match the supplied inputs")
    return receipt


def encode(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def t17_receipt(mode: str = "release") -> dict:
    """The T17 verifier's own fixture with the values the live writer hard-codes."""
    return {**T17._fixture(mode), "clean_machine": False, "hosted_ci_green": False, "security_ci_green": False,
            "hosted_ci_run_id": "absent", "security_ci_run_id": "absent", "claim_eligible": False}


def t19_receipt(mode: str = "release") -> dict:
    """A writer-shaped T19 receipt sharing commit, host and app artifact with t17_receipt."""
    source = t17_receipt(mode)
    value = {key: source[key] for key in T19.REQUIRED.intersection(source)}
    value.update(schema_version=T19.SCHEMA, gate_id=T19.GATE_ID, tier=T19.TIER, job_id="t19-self-test",
                 started_at="2026-09-01T01:00:00Z", finished_at="2026-09-01T01:02:00Z")
    value.update({field: "ab" * 32 for field in T19.HASH_FIELDS})
    value.update({field: source["expected_runs"] for field in T19.STAGE_FIELDS})
    return value


def fixtures() -> dict:
    """Synthetic eligible inputs shared by --self-test and the contract tests."""
    commit = "1" * 40
    t17, t19 = t17_receipt(), t19_receipt()

    def run(kind: str, run_id: str, event: str, checked: str) -> dict:
        names = CI.KINDS[kind]
        return {"schema_version": CI.SCHEMA, "repository": CI.REPOSITORY, "workflow_name": names["workflow_name"],
                "workflow_path": names["workflow_path"], "run_id": run_id, "run_attempt": 1, "head_sha": commit,
                "checked_source_sha": checked, "event": event, "status": "completed", "conclusion": "success",
                "html_url": f"https://github.com/{CI.REPOSITORY}/actions/runs/{run_id}",
                "fetched_at": "2026-09-02T00:00:00Z"}
    clean = {"schema_version": CLEAN_HOST_SCHEMA, "criterion": "A9", "commit": commit,
             "host_model": t17["host_model"], "macos_version": t17["macos_version"],
             "attested_at": "2026-08-31T12:00:00Z", "attested_by": "release-operator",
             "clean_machine_definition": "synthetic self-test statement; no host was inspected"}
    return {"commit": commit, "t17": t17, "t19": t19, "clean": clean,
            "hosted_full": run("hosted-full", "1001", "workflow_dispatch", commit),
            "security_quality": run("security-quality", "1002", "push", "absent")}


def self_test() -> int:
    base = fixtures()
    checks = 0

    def inputs(values: dict, *, with_clean: bool = True) -> dict:
        pairs = {key: (encode(values[key]), values[key]) for key in ("t17", "t19", "hosted_full", "security_quality")}
        if with_clean:
            pairs["clean"] = (encode(values["clean"]), values["clean"])
            pairs["clean_sha256"] = _sha(encode(values["clean"]))
        return pairs

    def outcome(mutate, *, with_clean: bool = True) -> object:
        nonlocal checks
        values = copy.deepcopy(base)
        mutate(values)
        checks += 1
        try:
            return build(commit=base["commit"], **inputs(values, with_clean=with_clean))
        except ClaimError as error:
            return error

    claim = outcome(lambda values: None)
    assert isinstance(claim, dict) and claim["claim_eligible"] and claim["unmet_conditions"] == [], claim
    pairs = inputs(base)
    pairs.pop("clean_sha256")
    verify(json.loads(encode(claim)), **pairs)
    ineligible = {
        "t17-campaign-not-release": lambda v: v.update(t17=t17_receipt("pilot")),
        "t19-signing-not-developer-id-notarized": lambda v: v["t19"].update(artifact_signing_class="development-signed"),
        "hosted-full-ci-not-success": lambda v: v["hosted_full"].update(conclusion="failure"),
        "security-quality-ci-not-success": lambda v: v["security_quality"].update(conclusion="cancelled"),
        "clean-host-attestation-absent": None,
    }
    for code, mutate in ineligible.items():
        result = outcome(mutate or (lambda values: None), with_clean=mutate is not None)
        assert isinstance(result, dict) and not result["claim_eligible"] and code in result["unmet_conditions"], (code, result)
    assert result["unmet_conditions"] == ["clean-host-attestation-absent"] and result["clean_machine"] is False
    refused = (
        lambda v: v["t19"].update(commit="2" * 40, tested_commit="2" * 40),
        lambda v: v["hosted_full"].update(head_sha="2" * 40),
        lambda v: v["security_quality"].update(event="pull_request"),
        lambda v: v.update(t17=copy.deepcopy(v["t19"])),
        lambda v: v["t17"].update(hosted_ci_run_id="999"),
        lambda v: v["t19"].update(macos_version="26.6"),
        lambda v: v["clean"].update(attested_at="2026-09-01T00:00:01Z"),
        lambda v: v["t17"].update(three_d_injection=True),
    )
    for mutate in refused:
        assert isinstance(outcome(mutate), ClaimError), mutate
    checks += 1
    try:
        build(commit=base["commit"], **{**inputs(base), "clean_sha256": "0" * 64})
    except ClaimError:
        pass
    else:
        raise AssertionError("an unbound clean-host attestation was accepted")
    tampered = json.loads(encode(claim))
    tampered["journeys"]["t17"]["receipt_sha256"] = "0" * 64
    for receipt, supplied in ((tampered, pairs), (claim, {**pairs, "t19": (encode(base["t19"]) + b" ", base["t19"])})):
        checks += 1
        try:
            verify(receipt, **supplied)
        except ClaimError:
            continue
        raise AssertionError("verify accepted a tampered receipt or input")
    print(f"PASS: A9 release claim builder and verifier ({checks} checks)")
    return 0


def _add_inputs(parser: argparse.ArgumentParser) -> None:
    for name in ("t17", "t19", "hosted-full-ci", "security-quality-ci"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--clean-host-attestation", type=Path)
    parser.add_argument("--require-claim-eligible", action="store_true")


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    make = commands.add_parser("build", help="derive the combined receipt from bound inputs")
    make.add_argument("--commit", required=True)
    _add_inputs(make)
    make.add_argument("--clean-host-attestation-sha256")
    make.add_argument("--out", type=Path, required=True)
    check = commands.add_parser("verify", help="rebuild from the inputs and compare exactly")
    check.add_argument("receipt", type=Path)
    check.add_argument("--expected-commit")
    _add_inputs(check)
    args = parser.parse_args()
    try:
        supplied = {key: read_input(getattr(args, attribute), label) for key, attribute, label in (
            ("t17", "t17", "T17 receipt"), ("t19", "t19", "T19 receipt"),
            ("hosted_full", "hosted_full_ci", "hosted-full CI attestation"),
            ("security_quality", "security_quality_ci", "security-quality CI attestation"))}
        if args.clean_host_attestation is not None:
            supplied["clean"] = read_input(args.clean_host_attestation, "clean-host attestation")
        if args.command == "build":
            receipt = build(commit=args.commit, clean_sha256=args.clean_host_attestation_sha256, **supplied)
        else:
            receipt = verify(read_input(args.receipt, "A9 claim receipt")[1], expected_commit=args.expected_commit, **supplied)
        if args.require_claim_eligible and not receipt["claim_eligible"]:
            raise ClaimError(f"receipt is consistent but not claim-eligible; unmet: {', '.join(receipt['unmet_conditions'])}")
        if args.command == "build":
            args.out.parent.mkdir(parents=True, exist_ok=True)
            with args.out.open("xb") as output:
                output.write(encode(receipt))
    except (OSError, ClaimError) as error:
        print(f"FAIL: A9 release claim: {error}", file=sys.stderr)
        return 1
    state = "claim-eligible" if receipt["claim_eligible"] else f"not claim-eligible (unmet: {', '.join(receipt['unmet_conditions'])})"
    print(f"PASS: A9 release claim {'written' if args.command == 'build' else 'verified'} for {receipt['commit']}; {state}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
