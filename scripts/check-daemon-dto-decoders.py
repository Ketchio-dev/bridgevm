#!/usr/bin/env python3
"""Keep the app's hand-written daemon DTO decoders internally consistent.

``VirtualMachineClient.swift`` decodes the daemon's JSON with hand-written
``init(from:)`` bodies rather than the compiler-generated ones, because several
fields accept both camelCase and snake_case spellings and some need unit
conversion. That hand-written form has already produced one real defect: the
resources decoder was rewritten after it stopped type-checking on CI, and it
had no test at all.

A ``CodingKeys`` case that no ``forKey:`` ever names is the failure mode with
no symptom. The field simply never arrives, the decode still succeeds, and the
UI shows a default. This checks that every declared key is decoded and that no
decode names a key that was never declared.

Decoders that pass their key list to a helper (``decodeFlexibleDate(keys:)``,
``firstInt``) are exempt, since there the keys are values rather than literal
``forKey:`` arguments; their coverage comes from unit tests instead.

    python3 scripts/check-daemon-dto-decoders.py
"""

from __future__ import annotations

import pathlib
import re
import sys

CLIENT = (
    pathlib.Path(__file__).resolve().parent.parent
    / "apps/macos/Sources/BridgeVMApp/Services/VirtualMachineClient.swift"
)

# Decoders that hand their CodingKeys to a helper as a list. Their keys are not
# written as `forKey: .name`, so the textual check cannot see them.
HELPER_CALL = re.compile(r"decodeFlexibleDate\(keys:|firstInt\(|firstString\(|firstBool\(")

STRUCT = re.compile(r"struct (\w+): Decodable \{(.*?)\n\}\n", re.S)
CODING_KEYS = re.compile(r"enum CodingKeys[^{]*\{(.*?)\n  \}", re.S)
INIT_FROM = re.compile(r"init\(from decoder: Decoder\) throws \{(.*?)\n  \}", re.S)


def main() -> int:
    if not CLIENT.exists():
        print(f"daemon DTO decoders: FAIL (missing {CLIENT})", file=sys.stderr)
        return 1

    source = CLIENT.read_text(errors="ignore")
    checked = 0
    problems: list[str] = []

    for struct in STRUCT.finditer(source):
        name, body = struct.group(1), struct.group(2)
        keys = CODING_KEYS.search(body)
        decoder = INIT_FROM.search(body)
        if not keys or not decoder:
            continue
        if HELPER_CALL.search(decoder.group(1)):
            continue

        checked += 1
        declared = {m.group(1) for m in re.finditer(r"case (\w+)", keys.group(1))}
        decoded = {m.group(1) for m in re.finditer(r"forKey: \.(\w+)", decoder.group(1))}

        for key in sorted(declared - decoded):
            problems.append(
                f"{name}.{key} is declared in CodingKeys but never decoded, "
                "so that field silently stays at its default"
            )
        for key in sorted(decoded - declared):
            problems.append(f"{name} decodes .{key}, which is not declared in CodingKeys")

    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        print(f"daemon DTO decoders: FAIL ({len(problems)})", file=sys.stderr)
        return 1

    if checked == 0:
        print(
            "daemon DTO decoders: FAIL (matched no decoders; the pattern has drifted)",
            file=sys.stderr,
        )
        return 1

    print(f"daemon DTO decoders: PASS ({checked} hand-written decoders)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
