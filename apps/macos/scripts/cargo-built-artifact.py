#!/usr/bin/env python3
"""Build one native Cargo target and return its reported executable path."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def fail(message: str) -> int:
    print(f"cargo artifact: {message}", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--kind", required=True, choices=("bin", "example"))
    parser.add_argument("cargo_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    cargo_args = args.cargo_args[1:] if args.cargo_args[:1] == ["--"] else args.cargo_args
    if not cargo_args or cargo_args[0] != "build":
        return fail("expected cargo build arguments")
    if any(arg in ("--target", "--manifest-path", "--message-format") or
           arg.startswith(("--target=", "--manifest-path=", "--message-format="))
           for arg in cargo_args[1:]):
        return fail("Cargo target, manifest, and message format are fixed by packaging")
    if os.environ.get("CARGO_BUILD_TARGET"):
        return fail("CARGO_BUILD_TARGET is unsupported for native app packaging")
    root = args.root.resolve()
    manifest = root / "Cargo.toml"
    if not manifest.is_file():
        return fail(f"workspace manifest is missing: {manifest}")
    packages = [cargo_args[index + 1] for index, arg in enumerate(cargo_args[:-1])
                if arg in ("-p", "--package")]
    if len(packages) != 1:
        return fail("expected exactly one -p package")
    metadata = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1",
         "--manifest-path", str(manifest)],
        cwd=root, stdout=subprocess.PIPE, text=True, check=False)
    if metadata.returncode:
        return metadata.returncode
    try:
        members = json.loads(metadata.stdout)["packages"]
        selected = [package for package in members if package["name"] == packages[0]]
        if len(selected) != 1:
            return fail(f"expected one metadata package named {packages[0]}")
        package_id = selected[0]["id"]
        targets = selected[0]["targets"]
        if not any(target["name"] == args.target and args.kind in target["kind"]
                   for target in targets):
            return fail("requested target is absent from the selected package")
    except (KeyError, TypeError, json.JSONDecodeError):
        return fail("Cargo metadata was malformed")
    command = ["cargo", *cargo_args, "--manifest-path", str(manifest), "--message-format=json"]
    process = subprocess.Popen(command, cwd=root, stdout=subprocess.PIPE, text=True)
    matches = []
    malformed = False
    assert process.stdout is not None
    for line in process.stdout:
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            malformed = True
            continue
        if message.get("reason") == "compiler-message":
            rendered = message.get("message", {}).get("rendered")
            if rendered:
                print(rendered, file=sys.stderr, end="")
        if message.get("reason") == "compiler-artifact":
            target = message.get("target", {})
            if (message.get("package_id") == package_id and
                    target.get("name") == args.target and args.kind in target.get("kind", [])):
                matches.append(message.get("executable"))
    result = process.wait()
    if result:
        return result if result > 0 else 1
    if malformed:
        return fail("Cargo emitted non-JSON build output")
    if len(matches) != 1:
        return fail(f"expected one {args.kind} executable named {args.target}; found {len(matches)}")
    executable = matches[0]
    if not isinstance(executable, str) or not Path(executable).is_absolute():
        return fail("Cargo did not report an absolute executable path")
    path = Path(executable)
    if not path.is_file() or not os.access(path, os.X_OK):
        return fail(f"Cargo-reported executable is missing or not executable: {path}")
    architecture = subprocess.run(
        ["/usr/bin/lipo", str(path), "-verify_arch", "arm64"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    if architecture.returncode:
        return fail(f"Cargo-reported executable requires an arm64 Mach-O slice: {path}")
    print(path)
    return 0


if __name__ == "__main__":
    sys.exit(main())
