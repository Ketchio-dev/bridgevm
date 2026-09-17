#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-import-contract.XXXXXX")"
trap 'chmod -R u+w "$TMP" 2>/dev/null || true; rm -rf "$TMP"' EXIT
MANIFEST="$ROOT/scripts/live-gates/windows-import-product-e2e-manifest.py"
REQUEST="$ROOT/scripts/live-gates/make-windows-import-product-e2e-request.py"
APP="$TMP/BridgeVM.app"; HELPER="$APP/Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
mkdir -p "$APP/Contents/MacOS" "$(dirname "$HELPER")" "$APP/Contents/Resources/target/release"
printf app > "$APP/Contents/MacOS/BridgeVMControl"; printf helper > "$HELPER"
printf runner > "$APP/Contents/Resources/target/release/hvf-runner"
printf disk > "$TMP/windows.raw"; truncate -s 67108864 "$TMP/vars.fd"
mkdir "$TMP/vtpm"; printf state > "$TMP/vtpm/tpm2-00.permall"; printf lock > "$TMP/vtpm/.lock"
INPUT="$TMP/input.tsv"
python3 - "$ROOT" "$APP" "$TMP" "$INPUT" <<'PY'
import importlib.util, pathlib, sys
root, app, temp, output = map(pathlib.Path, sys.argv[1:])
script = root / "scripts/live-gates/windows-import-product-e2e-manifest.py"
spec = importlib.util.spec_from_file_location("manifest", script); module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
assets = {
    "app_bundle": app, "app_executable": app / "Contents/MacOS/BridgeVMControl",
    "product_helper": app / "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E",
    "runner": app / "Contents/Resources/target/release/hvf-runner", "source_disk": temp / "windows.raw",
    "source_vars": temp / "vars.fd", "source_vtpm": temp / "vtpm",
}
with output.open("x") as out:
    out.write("campaign_mode\tpilot\n")
    for key, path in assets.items():
        digest = module.tree_hash(path, allow_symlinks=key == "app_bundle") if path.is_dir() else module.file_hash(path)
        out.write(f"{key}\t{path}\t{digest}\n")
PY
python3 "$MANIFEST" --manifest "$INPUT" --out "$TMP/verified.json"
LANE="/tmp/bridgevm-import-e2e-contract-$$"; rm -rf "$LANE"; mkdir -p "$LANE/inputs/vtpm"
trap 'chmod -R u+w "$TMP" "$LANE" 2>/dev/null || true; rm -rf "$TMP" "$LANE"' EXIT
cp "$TMP/windows.raw" "$LANE/inputs/windows.raw"; cp "$TMP/vars.fd" "$LANE/inputs/vars.fd"
cp -R "$TMP/vtpm/." "$LANE/inputs/vtpm/"; chmod -R a-w "$LANE/inputs"
nonce="$(printf 'a%.0s' {1..64})"; request="$LANE/request.json"
python3 "$REQUEST" --out "$request" --verified "$TMP/verified.json" --job-id import-contract \
  --commit "$(printf 'b%.0s' {1..40})" --mode pilot --lane 1 --nonce "$nonce" --lane-root "$LANE"
python3 - "$request" "$LANE" <<'PY'
import json, pathlib, sys
value = json.load(open(sys.argv[1])); root = pathlib.Path(sys.argv[2]); prefix = "a" * 12
assert value["schema_version"] == "bridgevm.windows-hvf-import-product-e2e-request.v1"
assert value["three_d_injection"] is False and value["source_disk_path"] == str(root / "inputs/windows.raw")
assert value["disk_path"] == str(root / "library" / f"bridgevm-a9-import-lane-1-{prefix}" / "bundle/disks/hvf-target.raw")
assert len(value) == 23
PY
rm "$request"; chmod u+w "$LANE/inputs/vars.fd"
if python3 "$REQUEST" --out "$request" --verified "$TMP/verified.json" --job-id import-contract \
  --commit "$(printf 'b%.0s' {1..40})" --mode pilot --lane 1 --nonce "$nonce" --lane-root "$LANE" >/dev/null 2>&1; then
  echo "writable vars clone was accepted" >&2; exit 1
fi
printf changed >> "$TMP/windows.raw"
if python3 "$MANIFEST" --manifest "$INPUT" --out "$TMP/changed.json" >/dev/null 2>&1; then
  echo "mutated canonical source was accepted" >&2; exit 1
fi
python3 "$ROOT/tests/integration/windows-import-product-e2e-receipt.py"; echo "PASS: installed-disk import product E2E manifest and request contracts"
