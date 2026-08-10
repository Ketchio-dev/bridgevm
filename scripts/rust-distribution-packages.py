#!/usr/bin/env python3
"""Emit cargo-metadata package records reachable by shipped Rust roots."""

import json
import subprocess

ROOT_NAMES = {"bridgevm-cli", "hvf-runner", "bridgevm-hvf"}
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], text=True
    )
)
packages = {package["id"]: package for package in metadata["packages"]}
resolve = {node["id"]: node for node in metadata["resolve"]["nodes"]}
stack = [package["id"] for package in metadata["packages"] if package["name"] in ROOT_NAMES]
reachable = set()
while stack:
    package_id = stack.pop()
    if package_id in reachable:
        continue
    reachable.add(package_id)
    for dependency in resolve[package_id]["deps"]:
        kinds = {kind["kind"] for kind in dependency["dep_kinds"]}
        if kinds <= {None, "build"}:
            stack.append(dependency["pkg"])
workspace = set(metadata["workspace_members"])
result = [packages[item] for item in reachable if item not in workspace]
print(json.dumps(sorted(result, key=lambda package: package["id"])))
