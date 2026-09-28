#!/usr/bin/env python3
"""Exercise release object selection with declared test and shipping targets."""
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
SENTINELS = "BRIDGEVM_REPO_ROOT\nBRIDGEVM_SWTPM_BIN\n/usr/local/bin/swtpm\n"
TARGETS = [
    {"name": "BridgeVMControl", "type": "executable"},
    {"name": "BridgeVMControlTests", "type": "test"},
    {"name": "OtherTests", "type": "test"},
    {"name": "ShippingTests", "type": "executable"},
]
PRODUCTS = [{"name": name, "type": {"executable": None}, "targets": [name]}
            for name in ("BridgeVMControl", "ShippingTests")]
def run_gate(root: Path, manifest: Path, strict: bool = False) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.update(PATH=f"{root / 'bin'}{os.pathsep}{env['PATH']}", BRIDGEVM_TEST_PACKAGE=str(manifest))
    return subprocess.run(
        ["bash", str(root / "scripts/check-release-overrides.sh")] + (["--require-artifacts"] if strict else []),
        cwd=root, env=env, capture_output=True, text=True, check=False,
    )

def require(result: subprocess.CompletedProcess[str], passes: bool, text: str) -> None:
    output = result.stdout + result.stderr
    if (result.returncode == 0) != passes or text not in output:
        raise AssertionError(f"expected pass={passes} containing {text!r}; got {output[-800:]!r}")

def object_path(root: Path, layout: str, config: str, target: str) -> Path:
    if layout == "modern":
        return (root / "apps/macos/.build/out/Intermediates.noindex/BridgeVMApp.build"
                / config.title() / f"{target}-p.build/Objects-normal/arm64" / f"{target}.o")
    return (root / "apps/macos/.build/arm64-apple-macosx"
            / config / f"{target}.build" / f"{target}.o")
def check_layout(layout: str) -> None:
    with tempfile.TemporaryDirectory(prefix=f"release-selector-{layout}-") as directory:
        root = Path(directory)
        (root / "scripts").mkdir()
        (root / "bin").mkdir()
        for filename in ("check-release-overrides.sh", "release_override_object_paths.py"):
            shutil.copy2(ROOT / "scripts" / filename, root / "scripts" / filename)
        swift = root / "bin/swift"
        swift.write_text('#!/bin/sh\n[ "$1" = package ] && [ "$2" = --package-path ] && [ "$4" = dump-package ] || exit 2\ncat "$BRIDGEVM_TEST_PACKAGE"\n')
        swift.chmod(0o755)
        manifest = root / "package.json"
        manifest.write_text(json.dumps({"targets": TARGETS, "products": PRODUCTS}))
        objects: dict[tuple[str, str], Path] = {}
        for config in ("debug", "release"):
            for target in ("BridgeVMControl", "BridgeVMControlTests", "ShippingTests"):
                path = object_path(root, layout, config, target)
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(SENTINELS if target == "BridgeVMControlTests" or
                                (config == "debug" and target == "BridgeVMControl") else "safe-object\n")
                objects[config, target] = path
        require(run_gate(root, manifest), True, "release overrides: PASS (3 overrides debug-only)")
        strings = root / "bin/strings"
        strings.write_text('#!/bin/sh\ncase "$*" in */Release/*|*/release/*) echo synthetic-strings-read-error >&2; exit 2;; esac\nexec /usr/bin/strings "$@"\n')
        strings.chmod(0o755)
        require(run_gate(root, manifest, True), False, "synthetic-strings-read-error")
        strings.unlink()
        shipping = objects["release", "ShippingTests"]
        shipping.write_text("BRIDGEVM_REPO_ROOT\n")
        require(run_gate(root, manifest), False, "BRIDGEVM_REPO_ROOT is reachable in the release build")
        shipping.unlink()
        require(run_gate(root, manifest), False, "missing executable product objects: ShippingTests")
        shipping.write_text("safe-object\n")
        product = objects["release", "BridgeVMControl"]
        product.write_text("BRIDGEVM_SWTPM_BIN\n")
        require(run_gate(root, manifest), False, "BRIDGEVM_SWTPM_BIN is reachable in the release build")
        product.write_text("safe-object\n")
        manifest.write_text(json.dumps({"products": PRODUCTS, "targets": [
            {**target, "type": "executable"} if target["name"] == "BridgeVMControlTests" else target
            for target in TARGETS
        ]}))
        require(run_gate(root, manifest), False, "BRIDGEVM_REPO_ROOT is reachable in the release build")
        manifest.write_text(json.dumps({"targets": TARGETS, "products": PRODUCTS}))
        debug_product = objects["debug", "BridgeVMControl"]
        debug_product.write_text("BRIDGEVM_REPO_ROOT\nBRIDGEVM_SWTPM_BIN\n")
        require(run_gate(root, manifest), False, "/usr/local/bin/swtpm is absent from the debug build too")
        debug_product.write_text(SENTINELS)
        manifest.write_text("{broken")
        require(run_gate(root, manifest), False, "SwiftPM target inventory could not be parsed")
        manifest.write_text(json.dumps({"targets": TARGETS, "products": PRODUCTS}))
        for target in ("BridgeVMControl", "ShippingTests"):
            objects["release", target].unlink()
        require(run_gate(root, manifest), False, "no product objects")
        objects["release", "BridgeVMControlTests"].unlink()
        require(run_gate(root, manifest), True, "release overrides: SKIP (artifacts absent)")
        require(run_gate(root, manifest, True), False, "release overrides: FAIL (artifacts absent)")

for layout_name in ("modern", "legacy"):
    check_layout(layout_name)
print("release override object selection: PASS (modern and legacy positive/negative controls)")
