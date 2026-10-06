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
import os
import argparse
import json
import re
import sys
import tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from receipt_public_fields import ALLOWED_FIELDS

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
SAFE_CONFOUNDER_VALUES = frozenset(("full clone integrity hash immediately precedes boot (warm cache)", "boot harness omits product vTPM, clipboard/share, and long-lived app session", "guest-unbuffered sequential I/O uses a host-warm backing image", "storage and desktop timing share one end-to-end live attempt", "a host power-source event monitor runs concurrently"))
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
                if isinstance(item, (dict, list)):
                    raise RedactionError(f"field {key!r} contains nested data")
                if _value_is_secret(item) and not (key == "known_confounders" and item in SAFE_CONFOUNDER_VALUES):
                    raise RedactionError(f"field {key!r} carries a non-publishable value")
        if isinstance(value, dict):
            raise RedactionError(f"field {key!r} is nested; the allowlist is flat")
        public[key] = value
    return public

def _write_new(path: Path, rendered: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as output:
        output.write(rendered)

def _self_test() -> int:
    checks = 0
    def check(condition: bool, description: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            print(f"FAIL: {description}", file=sys.stderr)
            raise SystemExit(1)

    def refuses(payload: object, description: str) -> None:
        try:
            redact(payload)  # type: ignore[arg-type]
        except RedactionError:
            check(True, description)
        else:
            check(False, description)

    out = redact({"probe": "hvf_vtimer_cancel", "iterations": 10000, "pass": True,
                  "baseline_matches": 20})
    check(out["probe"] == "hvf_vtimer_cancel", "an allowed string field is kept")
    check(out["iterations"] == 10000, "an allowed numeric field is kept")
    check(out["pass"] is True and out["baseline_matches"] == 20, "allowed result fields are kept")
    hashes = redact({"ppsspp_payload_sha256": "ab" * 32, "ppsspp_executable_sha256": "cd" * 32, "renderer_sha256": "34" * 32, "workload_script_sha256": "56" * 32,
                     "firmware_sha256": "ef" * 32, "harness_commit": "1" * 40, "campaign_id": "2" * 32, "workload_profile": "shipping-core-3d-boot-v1", "render_server_sha256": "78" * 32})
    check(len(hashes) == 9 and hashes["render_server_sha256"] == "78" * 32, "artifact, server, harness, campaign, and workload identities are kept")
    safe_confounders = list(SAFE_CONFOUNDER_VALUES)
    check(redact({"known_confounders": safe_confounders})["known_confounders"] == safe_confounders, "fixed public confounders are kept")
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
        refuses({"image_hash": bad}, f"{bad!r} is refused"); refuses({"render_server_sha256": bad}, "private server identity is refused")

    # A real hash under the same key is fine.
    out = redact({"image_hash": "sha256:" + "ab" * 32})
    check("image_hash" in out, "a genuine hash is publishable")

    # A list carrying a secret is refused, not silently trimmed.
    refuses({"known_confounders": ["ok", "/Users/me/win.vhdx"]}, "a secret inside a list is refused")

    # Nesting is refused because the allowlist cannot vouch for its contents.
    refuses({"probe": "x", "outcome": {"nested": "value"}}, "a nested object is refused")
    for nested in ([{"nested": "value"}], [["nested"]]):
        refuses({"evidence_paths": nested}, "a nested container inside a list is refused")
    refuses([], "a non-object receipt is refused")

    with tempfile.TemporaryDirectory(prefix="bridgevm-redactor-") as temporary:
        victim = Path(temporary) / "victim"
        linked = Path(temporary) / "public.json"
        victim.write_text("do-not-overwrite", encoding="utf-8")
        os.link(victim, linked)
        try:
            _write_new(linked, "replacement")
        except FileExistsError:
            check(victim.read_text(encoding="utf-8") == "do-not-overwrite", "an existing hardlink output is refused")
        else:
            check(False, "an existing hardlink output is refused")

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
        try:
            _write_new(args.destination, rendered)
        except OSError as error:
            print(f"refusing output {args.destination}: {error}", file=sys.stderr)
            return 1
    else:
        sys.stdout.write(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
