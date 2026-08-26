#!/usr/bin/env python3
"""Redact a live-gate receipt down to an allowlist of publishable fields.

Live gates run against private Windows media: disk images, UEFI variable
stores, vTPM state and guest credentials. A receipt is evidence and gets
attached to issues and pull requests, so it must not carry any of that.

The policy is an allowlist, not a denylist. A field nobody has explicitly
declared publishable is dropped, because the failure mode of a denylist is
silent disclosure of whatever was added last.

Usage:
  redact-receipt.py --in receipt.json --out public.json
  redact-receipt.py --self-test
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# Fields safe to publish. Anything absent is dropped.
ALLOWED_FIELDS = frozenset(
    {
        "probe",
        "gate_id",
        "criterion",
        "tested_commit",
        "tier",
        "job_id",
        "commit",
        "image_sha256",
        "guest_image_sha256",
        "vars_sha256",
        "driver_store_hash",
        "title_sha256",
        "ppsspp_sha256",
        "ppsspp_payload_sha256",
        "ppsspp_executable_sha256",
        "dxvk_d3d11_sha256",
        "dxvk_dxgi_sha256",
        "virglrenderer_sha256",
        "moltenvk_sha256",
        "gate_asset_hash",
        "input_manifest_sha256",
        "started_at",
        "finished_at",
        "elapsed_ms",
        "host_model",
        "macos_version",
        "host_os",
        "host_hardware",
        "outcome",
        "pass",
        "passes",
        "failures",
        "sample_count",
        "run_count",
        "required_run_count",
        "threshold_fps",
        "module_identity_verified",
        "iterations",
        "timer_wakes",
        "canceled_exits",
        "surplus_canceled",
        "masked_past_deadline",
        "recoveries",
        "swallowed_unrecovered",
        "vtimer_exits",
        "trace_overflow",
        "boots_attempted",
        "boots_passed",
        "stage4_pass",
        "firstboot_fresh",
        "cpus_online",
        "fps_p50",
        "fps_samples",
        "evidence_paths",
        "known_confounders",
        "driver_hash",
        "image_hash",
        "vars_hash",
        "binary_hash", "injector_sha256", "agent_sha256", "prepared_image_sha256", "prepared_vars_sha256", "injector_boot_observed", "f1_driver_load", "f2_resize", "f3_window_verbs", "f4_glyph_observation", "active_scanout_capture", "landed", "p95_first_changed_ms", "baseline_iterations", "baseline_matches", "load_processes", "callback_errors", "frames_rendered", "sealed_package_sha256", "diagnostic_umd_sha256", "diagnostic_version", "installed_diagnostic_verified", "installed_umd_sha256", "driver_version", "diagnostic_correlation", "diagnostic_guest_event_count", "diagnostic_submit_event_count", "diagnostic_max_submit_allocations", "diagnostic_max_submit_capacity", "diagnostic_max_d3d_list_size", "diagnostic_host_event_count", "diagnostic_missing_create_attach_count", "diagnostic_guest_resource_id", "diagnostic_host_resource_id", "diagnostic_missing_create_resource_id",
    }
)

# A value that looks like one of these is refused outright, even under an
# allowed key: a hash field must carry a hash, not a path to the thing hashed.
SECRET_VALUE_PATTERNS = (
    re.compile(r"\.(qcow2|vhdx|vhd|img|raw|iso|fd)(\b|$)", re.IGNORECASE),
    re.compile(r"(^|/)(vars|varstore|nvram|efivars)[^/]*$", re.IGNORECASE),
    re.compile(r"\b(tpm|vtpm|swtpm)\b", re.IGNORECASE),
    re.compile(r"\b(password|passwd|secret|token|credential|private[_-]?key)\b", re.IGNORECASE),
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    re.compile(r"(^|/)(Users|home)/[^/]+/", re.IGNORECASE),
)


class RedactionError(ValueError):
    """A receipt carried a value that must never be published."""


def _value_is_secret(value: object) -> bool:
    if not isinstance(value, str):
        return False
    return any(pattern.search(value) for pattern in SECRET_VALUE_PATTERNS)


def redact(receipt: dict) -> dict:
    """Return the publishable subset of `receipt`.

    Raises `RedactionError` if an allowed field carries a secret-looking value,
    rather than publishing it or quietly blanking it. A receipt that trips this
    is a bug in whatever produced it and must be fixed there.
    """
    if not isinstance(receipt, dict):
        raise RedactionError("a receipt must be a JSON object")

    public: dict = {}
    for key, value in receipt.items():
        if key not in ALLOWED_FIELDS:
            continue
        if _value_is_secret(value):
            raise RedactionError(f"field {key!r} carries a non-publishable value")
        if isinstance(value, list):
            for item in value:
                if _value_is_secret(item):
                    raise RedactionError(f"field {key!r} carries a non-publishable value")
        if isinstance(value, dict):
            raise RedactionError(f"field {key!r} is nested; the allowlist is flat")
        public[key] = value
    return public


def _self_test() -> int:
    checks = 0

    def check(condition: bool, description: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            print(f"FAIL: {description}", file=sys.stderr)
            raise SystemExit(1)

    out = redact({"probe": "hvf_vtimer_cancel", "iterations": 10000, "pass": True,
                  "baseline_matches": 20, "callback_errors": 0})
    check(out["probe"] == "hvf_vtimer_cancel", "an allowed string field is kept")
    check(out["iterations"] == 10000, "an allowed numeric field is kept")
    check(out["pass"] is True and out["baseline_matches"] == 20 and out["callback_errors"] == 0, "allowed result fields are kept")
    hashes = redact({"ppsspp_payload_sha256": "ab" * 32, "ppsspp_executable_sha256": "cd" * 32})
    check(len(hashes) == 2, "PPSSPP payload and executable identities are kept")

    # Unknown fields are dropped rather than published.
    out = redact({"probe": "x", "disk_path": "/Users/me/win.qcow2"})
    check("disk_path" not in out, "an unknown field is dropped")
    check(list(out) == ["probe"], "only allowlisted fields survive")

    # A field added later without being declared is also dropped: this is the
    # property a denylist would not give.
    out = redact({"probe": "x", "some_future_field": "anything"})
    check("some_future_field" not in out, "an undeclared new field is dropped")

    # Secret-looking values under an allowed key are refused loudly.
    for bad in (
        "/Users/insighton/BridgeVM/win11.qcow2",
        "/var/db/bridgevm/OVMF_VARS.fd",
        "swtpm state at /tmp/vtpm",
        "password=hunter2",
        "-----BEGIN RSA PRIVATE KEY-----",
        "/Users/insighton/secret-notes",
    ):
        try:
            redact({"image_hash": bad})
        except RedactionError:
            checks += 1
        else:
            print(f"FAIL: {bad!r} should have been refused", file=sys.stderr)
            raise SystemExit(1)

    # A real hash under the same key is fine.
    out = redact({"image_hash": "sha256:" + "ab" * 32})
    check("image_hash" in out, "a genuine hash is publishable")

    # A list carrying a secret is refused, not silently trimmed.
    try:
        redact({"failures": ["ok", "/Users/me/win.vhdx"]})
    except RedactionError:
        checks += 1
    else:
        print("FAIL: a secret inside a list should be refused", file=sys.stderr)
        raise SystemExit(1)

    # Nesting is refused because the allowlist cannot vouch for its contents.
    try:
        redact({"probe": "x", "outcome": {"nested": "value"}})
    except RedactionError:
        checks += 1
    else:
        print("FAIL: a nested object should be refused", file=sys.stderr)
        raise SystemExit(1)

    try:
        redact([])  # type: ignore[arg-type]
    except RedactionError:
        checks += 1
    else:
        print("FAIL: a non-object receipt should be refused", file=sys.stderr)
        raise SystemExit(1)

    print(f"PASS: redact-receipt self-test ({checks} checks)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--in", dest="source", type=Path)
    parser.add_argument("--out", dest="destination", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return _self_test()
    if not args.source:
        parser.error("--in is required unless --self-test is given")

    receipt = json.loads(args.source.read_text())
    try:
        public = redact(receipt)
    except RedactionError as error:
        print(f"refusing to publish {args.source}: {error}", file=sys.stderr)
        return 1

    rendered = json.dumps(public, indent=2, sort_keys=True) + "\n"
    if args.destination:
        args.destination.parent.mkdir(parents=True, exist_ok=True)
        args.destination.write_text(rendered)
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
