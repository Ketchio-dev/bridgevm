#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; TMP="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-import-tier.XXXXXX")"
trap 'chmod -R u+w "$TMP" 2>/dev/null || true; rm -rf "$TMP"' EXIT
TIER="$ROOT/scripts/live-gates/run-windows-import-product-e2e-tier.sh"; VERIFY="$ROOT/scripts/verify-windows-import-product-e2e-receipt.py"
APP="$TMP/BridgeVM.app"; RES="$APP/Contents/Resources"; HELPER_APP="$APP/Contents/Helpers/BridgeVMProductE2E.app"; HELPER="$HELPER_APP/Contents/MacOS/BridgeVMProductE2E"
mkdir -p "$APP/Contents/MacOS" "$HELPER_APP/Contents/MacOS" "$RES/target/release"
printf '#!/bin/sh\nexit 0\n' > "$APP/Contents/MacOS/BridgeVMControl"; printf '#!/bin/sh\nexit 0\n' > "$RES/target/release/hvf-runner"
printf '#!/bin/sh\nexec /usr/bin/python3 "$(dirname "$0")/../../../../Resources/fake-import-helper.py" "$@"\n' > "$HELPER"
chmod 755 "$APP/Contents/MacOS/BridgeVMControl" "$RES/target/release/hvf-runner" "$HELPER"
cp "$ROOT/tests/fixtures/fake-windows-import-product-e2e-helper.py" "$RES/fake-import-helper.py"
cp "$ROOT/apps/macos/BridgeVMProductE2E-Info.plist" "$HELPER_APP/Contents/Info.plist"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>BridgeVMControl</string><key>CFBundleIdentifier</key><string>dev.bridgevm.t19-fixture</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>
PLIST
codesign --force --sign - "$HELPER_APP" >/dev/null; codesign --force --sign - "$APP" >/dev/null
printf installed > "$TMP/windows.raw"; truncate -s 67108864 "$TMP/vars.fd"; mkdir "$TMP/vtpm"; printf state > "$TMP/vtpm/tpm2-00.permall"; printf lock > "$TMP/vtpm/.lock"
write_manifest() { python3 - "$ROOT" "$APP" "$TMP" "$1" "$2" <<'PY'
import importlib.util,pathlib,sys
root,app,temp,output,mode=map(pathlib.Path,sys.argv[1:]); spec=importlib.util.spec_from_file_location("m",root/"scripts/live-gates/windows-import-product-e2e-manifest.py"); m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
assets={"app_bundle":app,"app_executable":app/"Contents/MacOS/BridgeVMControl","product_helper":app/"Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E","runner":app/"Contents/Resources/target/release/hvf-runner","source_disk":temp/"windows.raw","source_vars":temp/"vars.fd","source_vtpm":temp/"vtpm"}
with output.open("x") as out:
 out.write(f"campaign_mode\t{mode}\n")
 for key,path in assets.items(): out.write(f"{key}\t{path}\t{m.tree_hash(path,allow_symlinks=key=='app_bundle') if path.is_dir() else m.file_hash(path)}\n")
PY
}
PILOT="$TMP/pilot.tsv"; write_manifest "$PILOT" pilot; OUT="$TMP/pilot-out"
"$TIER" --out "$OUT" --input-manifest "$PILOT" --job-id t19-fixture
"$VERIFY" "$OUT/receipt.json" --expected-commit "$(git -C "$ROOT" rev-parse HEAD)" >/dev/null
python3 - "$OUT/receipt.json" <<'PY'
import json,sys
r=json.load(open(sys.argv[1])); assert r["pass"] is True and r["run_count"]==1 and r["claim_eligible"] is False and r["three_d_injection"] is False
PY
BAD="$TMP/bad-out"
if "$TIER" --out "$BAD" --input-manifest "$PILOT" --job-id bad-hash-fixture >/dev/null 2>&1; then echo "bad lane passed" >&2; exit 1; fi
grep -q '"failure_code": "integration-failed"' "$BAD/receipt.json"
echo "PASS: installed-disk import live tier contracts"
