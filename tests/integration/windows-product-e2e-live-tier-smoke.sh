#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-t17.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
TIER="$ROOT/scripts/live-gates/run-windows-product-e2e-tier.sh"
MANIFEST_TOOL="$ROOT/scripts/live-gates/windows-product-e2e-manifest.py"
VERIFY="$ROOT/scripts/verify-windows-product-e2e-receipt.py"
PUBLISH="$ROOT/scripts/live-gates/publish-receipt.sh"
COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
checks=0
check() { local message="$1"; shift; "$@" || { echo "FAIL: $message" >&2; exit 1; }; checks=$((checks + 1)); }

python3 - "$ROOT" <<'PY'
import importlib.util, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("verify", root / "scripts/verify-windows-product-e2e-receipt.py")
verify = importlib.util.module_from_spec(spec); spec.loader.exec_module(verify)
schema = json.loads((root / "schemas/windows-hvf-3d-off-product-e2e-receipt-v1.json").read_text())
assert schema["additionalProperties"] is False
assert set(schema["required"]) == set(schema["properties"]) == set(verify.REQUIRED_FIELDS)
PY
checks=$((checks + 1))

missing_manifest="$TMP/missing-app.tsv"; missing_app="$TMP/Missing.app"
printf 'campaign_mode\tpilot\n' > "$missing_manifest"
printf 'app_bundle\t%s\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'app_executable\t%s/Contents/MacOS/BridgeVMControl\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'product_helper\t%s/Contents/MacOS/BridgeVMProductE2E\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'runner\t%s/Contents/Resources/target/release/hvf-runner\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'firmware\t%s/Contents/Resources/firmware/edk2-aarch64-secure-code.fd\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'secure_boot_policy\t%s/Contents/Resources/B/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'iso\t%s/missing.iso\t%064d\n' "$TMP" 0 >> "$missing_manifest"
printf 'bundled_vars_seed\t%s/Contents/Resources/B/windows-boot-seed-vars.fd.gz\t%064d\n' "$missing_app" 0 >> "$missing_manifest"
printf 'guest_payload\t%s/missing-guest-payload\t%064d\n' "$TMP" 0 >> "$missing_manifest"
printf 'guest_payload_manifest\t%s/missing-guest-payload.tsv\t%064d\n' "$TMP" 0 >> "$missing_manifest"
missing_out="$TMP/missing-out"
check "missing app blocks rather than inventing a run" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id missing-app >/dev/null 2>&1' _ "$TIER" "$missing_out" "$missing_manifest"
check "missing app leaves a valid explicit blocker" bash -c '"$1" "$2" --expected-commit "$3" >/dev/null && grep -q '"'"'"failure_code": "missing-app-artifact"'"'"' "$2" && grep -q '"'"'"run_count": 0'"'"' "$2"' _ "$VERIFY" "$missing_out/receipt.json" "$COMMIT"

APP="$TMP/BridgeVMControl.app"; RES="$APP/Contents/Resources"; MACOS="$APP/Contents/MacOS"
mkdir -p "$MACOS" "$RES/target/release" "$RES/firmware" "$RES/BridgeVMControl_BridgeVMControl.bundle"
printf '#!/bin/sh\nexit 0\n' > "$MACOS/BridgeVMControl"
printf '#!/bin/sh\nexec /usr/bin/python3 "$(dirname "$0")/../Resources/fake-product-helper.py" "$@"\n' > "$MACOS/BridgeVMProductE2E"
printf '#!/bin/sh\nexit 0\n' > "$RES/target/release/hvf-runner"
chmod 755 "$MACOS/BridgeVMControl" "$MACOS/BridgeVMProductE2E" "$RES/target/release/hvf-runner"
printf firmware > "$RES/firmware/edk2-aarch64-secure-code.fd"
python3 - "$RES/firmware/edk2-aarch64-secure-code.fd" "$RES/BridgeVMControl_BridgeVMControl.bundle/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json" <<'PY'
import hashlib,json,sys
firmware=hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest()
policy={"schemaVersion":1,"policy":"fixture-policy","source":{"tag":"v1","commit":"a"*40,"assetSha256":"b"*64},"firmware":{"fileName":"edk2-aarch64-secure-code.fd","sha256":firmware,"edk2Commit":"c"*40},"variables":[{"name":"PK","vendorGuid":"guid","attributes":39,"sha256":"d"*64}]}
open(sys.argv[2],"w").write(json.dumps(policy)+"\n")
PY
printf seed > "$RES/BridgeVMControl_BridgeVMControl.bundle/windows-boot-seed-vars.fd.gz"
cp "$ROOT/tests/fixtures/fake-windows-product-e2e-helper.py" "$RES/fake-product-helper.py"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>BridgeVMControl</string><key>CFBundleIdentifier</key><string>dev.bridgevm.t17-fixture</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>
PLIST
codesign --force --deep --sign - "$APP" >/dev/null
printf 'private Windows ISO fixture\n' > "$TMP/windows.iso"
GUEST_PAYLOAD="$TMP/guest-payload"; GUEST_PAYLOAD_MANIFEST="$TMP/guest-payload.tsv"
mkdir "$GUEST_PAYLOAD"; printf guest > "$GUEST_PAYLOAD/agent.bin"; printf 'agent.bin\tfixture\n' > "$GUEST_PAYLOAD_MANIFEST"

write_manifest() {
  python3 - "$ROOT" "$APP" "$1" "$2" "$3" "$GUEST_PAYLOAD" "$GUEST_PAYLOAD_MANIFEST" <<'PY'
import importlib.util, pathlib, sys
root, app, iso, mode, output, guest, guest_manifest = map(pathlib.Path, sys.argv[1:])
spec=importlib.util.spec_from_file_location("manifest",root/"scripts/live-gates/windows-product-e2e-manifest.py"); module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
bundle=app/"Contents/Resources/BridgeVMControl_BridgeVMControl.bundle"
assets={"app_bundle":app,"app_executable":app/"Contents/MacOS/BridgeVMControl","product_helper":app/"Contents/MacOS/BridgeVMProductE2E","runner":app/"Contents/Resources/target/release/hvf-runner","firmware":app/"Contents/Resources/firmware/edk2-aarch64-secure-code.fd","secure_boot_policy":bundle/"secureboot-microsoft-windows-transition-aarch64-v1.6.5.json","iso":iso,"bundled_vars_seed":bundle/"windows-boot-seed-vars.fd.gz","guest_payload":guest,"guest_payload_manifest":guest_manifest}
with output.open("x") as out:
    out.write(f"campaign_mode\t{mode}\n")
    for key,path in assets.items():
        digest=module.tree_hash(path,allow_symlinks=key=="app_bundle") if key in ("app_bundle","guest_payload") and path.is_dir() else (module.file_hash(path) if path.is_file() else "1"*64)
        out.write(f"{key}\t{path}\t{digest}\n")
PY
}
pilot_manifest="$TMP/pilot.tsv"; write_manifest "$TMP/windows.iso" pilot "$pilot_manifest"
missing_guest_manifest="$TMP/missing-guest.tsv"
python3 - "$pilot_manifest" "$missing_guest_manifest" "$TMP/missing-payload" <<'PY'
import sys
rows=open(sys.argv[1]).read().splitlines()
open(sys.argv[2],"w").write("\n".join((f"guest_payload\t{sys.argv[3]}\t"+row.split("\t")[2]) if row.startswith("guest_payload\t") else row for row in rows)+"\n")
PY
missing_guest_out="$TMP/missing-guest-out"
check "missing guest payload has its stable preflight blocker" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id missing-guest >/dev/null 2>&1 && grep -q '"'"'"failure_code": "missing-guest-payload"'"'"' "$2/receipt.json"' _ "$TIER" "$missing_guest_out" "$missing_guest_manifest"
pilot_out="$TMP/pilot-out"
check "sealed synthetic product transcript exercises the pilot scaffold" "$TIER" --out "$pilot_out" --input-manifest "$pilot_manifest" --job-id pilot-fixture
check "pilot is bounded pass evidence but never claim eligible" bash -c '"$1" "$2" --expected-commit "$3" >/dev/null && python3 - "$2" <<'"'"'PY'"'"'
import json,sys
r=json.load(open(sys.argv[1])); assert r["pass"] is True and r["run_count"]==1 and r["guest_evidence_sha256"]!="absent" and r["claim_eligible"] is False and r["criterion_pass"] is False and r["capability_promotion"] is False
PY' _ "$VERIFY" "$pilot_out/receipt.json" "$COMMIT"
check "dedicated publisher validates before and after exact redaction" "$PUBLISH" t17-windows-hvf-product-e2e "$pilot_out" "$ROOT" "$COMMIT"
check "public pilot contains no private path and equals the safe private receipt" bash -c 'cmp -s "$1/receipt.json" "$1/receipt.public.json" && ! grep -Fq "$2" "$1/receipt.public.json"' _ "$pilot_out" "$TMP"
release_manifest="$TMP/release.tsv"; write_manifest "$TMP/windows.iso" release "$release_manifest"
release_out="$TMP/release-out"
check "fixed release scaffold runs exactly three sequential lanes" "$TIER" --out "$release_out" --input-manifest "$release_manifest" --job-id release-fixture
check "release transcript is 3/3 but remains non-promoting without UI signing and hosted seals" bash -c 'python3 - "$1" <<'"'"'PY'"'"'
import json,sys
r=json.load(open(sys.argv[1])); assert (r["expected_runs"],r["run_count"],r["passes"],r["failures"])==(3,3,3,0); assert r["pass"] and r["ui_frontend_automated"] and not r["claim_eligible"]
PY' _ "$release_out/receipt.json"
check "release lanes use three distinct private roots" bash -c 'test "$(grep -h '^"'"'lane_root='"'"' "$1"/private/lane-*-helper.log | sort -u | wc -l | tr -d " ")" = 3' _ "$release_out"
crosslane_out="$TMP/crosslane-out"; check "cross-lane hardlink alias fails isolation" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id crosslane-fixture >/dev/null 2>&1 && grep -q '"'"'"failure_code": "integration-failed"'"'"' "$2/receipt.json"' _ "$TIER" "$crosslane_out" "$release_manifest"
missing_iso_manifest="$TMP/missing-iso.tsv"; write_manifest "$TMP/not-present.iso" pilot "$missing_iso_manifest"
missing_iso_out="$TMP/missing-iso-out"
check "missing ISO has its stable preflight blocker" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id missing-iso >/dev/null 2>&1 && grep -q '"'"'"failure_code": "missing-windows-iso"'"'"' "$2/receipt.json"' _ "$TIER" "$missing_iso_out" "$missing_iso_manifest"

changed_manifest="$TMP/changed.tsv"; write_manifest "$TMP/windows.iso" pilot "$changed_manifest"; printf changed >> "$TMP/windows.iso"
changed_out="$TMP/changed-out"
check "post-seal mutation blocks before product execution" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id changed-input >/dev/null 2>&1 && grep -q '"'"'"failure_code": "hash-mismatch"'"'"' "$2/receipt.json" && test ! -e "$2/private/lane-1-helper.log"' _ "$TIER" "$changed_out" "$changed_manifest"
printf 'private Windows ISO fixture\n' > "$TMP/windows.iso"

noresult_manifest="$TMP/noresult.tsv"; write_manifest "$TMP/windows.iso" pilot "$noresult_manifest"
noresult_out="$TMP/noresult-out"
check "zero-exit helper without a guest result cannot pass" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id noresult-fixture >/dev/null 2>&1 && grep -q '"'"'"failure_code": "product-model-failed"'"'"' "$2/receipt.json"' _ "$TIER" "$noresult_out" "$noresult_manifest"
survivor_out="$TMP/survivor-out"
check "helper survivor withholds cleanup and leaves a cleanup-failed receipt" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id survivor-fixture >/dev/null 2>&1 && grep -q '"'"'"failure_code": "cleanup-failed"'"'"' "$2/receipt.json" && root=$(sed -n '"'"'s/^lane_root=//p'"'"' "$2/private/lane-1-helper.log") && test -d "$root" && pgrep -f "$root" >/dev/null && pkill -TERM -f "$root" && work=${root%/lane-1} && case "$work" in /tmp/bridgevm-e2e-survivor-fixture.??????) rm -rf -- "$work";; *) exit 1;; esac' _ "$TIER" "$survivor_out" "$noresult_manifest"
malformed_out="$TMP/malformed-out"
check "unknown lane evidence fails closed" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id malformed-fixture >/dev/null 2>&1 && grep -q '"'"'"failure_code": "integration-failed"'"'"' "$2/receipt.json"' _ "$TIER" "$malformed_out" "$noresult_manifest"
for adversary in bad-hash alias guest-alias partial bad-snapshot bad-source-receipt bad-secure bad-guest bad-raw bad-audio bad-mutation; do adversary_out="$TMP/$adversary-out"; check "$adversary lane cannot produce PASS" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id "$4-fixture" >/dev/null 2>&1 && grep -q '"'"'"failure_code": "integration-failed"'"'"' "$2/receipt.json" && grep -q '"'"'"pass": false'"'"' "$2/receipt.json"' _ "$TIER" "$adversary_out" "$noresult_manifest" "$adversary"; done

duplicate_manifest="$TMP/duplicate.tsv"; cp "$noresult_manifest" "$duplicate_manifest"; printf 'campaign_mode\tpilot\n' >> "$duplicate_manifest"; duplicate_out="$TMP/duplicate-out"
check "duplicate manifest metadata blocks before product execution" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id duplicate-manifest >/dev/null 2>&1 && grep -q '"'"'"failure_code": "internal-error"'"'"' "$2/receipt.json" && test ! -e "$2/private/lane-1-helper.log"' _ "$TIER" "$duplicate_out" "$duplicate_manifest"

cancel_out="$TMP/cancel-out"; mkdir "$cancel_out"; : > "$cancel_out/cancel.requested"
check "pre-run cancellation writes a schema-valid canceled receipt" bash -c '! "$1" --out "$2" --input-manifest "$3" --job-id canceled-fixture >/dev/null 2>&1 && "$4" "$2/receipt.json" --expected-commit "$5" >/dev/null && grep -q '"'"'"outcome": "canceled"'"'"' "$2/receipt.json"' _ "$TIER" "$cancel_out" "$noresult_manifest" "$VERIFY" "$COMMIT"

bad_publish="$TMP/bad-publish"; mkdir "$bad_publish"; cp "$pilot_out/receipt.json" "$bad_publish/receipt.json"
python3 - "$bad_publish/receipt.json" <<'PY'
import json,sys
p=sys.argv[1]; value=json.load(open(p)); value["private_iso_path"]="C:\\Users\\private\\Windows.iso"; open(p,"w").write(json.dumps(value)+"\n")
PY
check "schema-invalid private receipt is withheld" bash -c '! "$1" t17-windows-hvf-product-e2e "$2" "$3" "$4" >/dev/null 2>&1 && test ! -e "$2/receipt.public.json"' _ "$PUBLISH" "$bad_publish" "$ROOT" "$COMMIT"

missing_receipt="$TMP/missing-receipt"; mkdir "$missing_receipt"; cp "$noresult_manifest" "$missing_receipt/input-manifest.tsv"
printf 'job_id=missing-receipt\ntier=t17-windows-hvf-product-e2e\ncommit=%s\nsubmitted_at=%s\n' "$COMMIT" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$missing_receipt/job.env"
check "worker-side missing receipt fallback is schema valid" "$ROOT/scripts/live-gates/write-windows-product-e2e-missing-receipt.sh" "$missing_receipt" "$ROOT" missing-receipt "$COMMIT"
check "missing fallback cannot claim success" bash -c '"$1" "$2" --expected-commit "$3" >/dev/null && grep -q '"'"'"failure_code": "missing-tier-receipt"'"'"' "$2"' _ "$VERIFY" "$missing_receipt/receipt.json" "$COMMIT"

stale_root="$TMP/stale-queue"; stale="$stale_root/running/stale-t17"; mkdir -p "$stale" "$stale_root/done" "$TMP/stale-work"; cp "$pilot_out/receipt.json" "$stale/receipt.json"; printf 'job_id=stale-t17\ntier=t17-windows-hvf-product-e2e\ncommit=%s\n' "$COMMIT" > "$stale/job.env"
check "stale T17 without its sealed worktree never gets publicly redacted" "$ROOT/scripts/live-gates/recover-stale-jobs.sh" "$ROOT" "$stale_root" "$TMP/stale-work"
check "stale T17 is retained as interrupted with its receipt withheld" bash -c 'test -f "$1/done/stale-t17/receipt.json" && test ! -e "$1/done/stale-t17/receipt.public.json" && grep -q '"'"'^receipt=withheld-no-sealed-worktree$'"'"' "$1/done/stale-t17/result.env"' _ "$stale_root"

queue_root="$TMP/queue"; queued_id="t17-submit-fixture"
check "queue accepts T17 only as a sealed-manifest tier without a fake binary" bash -c 'test "$(BRIDGEVM_LIVE_ROOT="$1" "$2" submit t17-windows-hvf-product-e2e --sha "$3" --input-manifest "$4" --job-id "$5")" = "$5" && test -f "$1/queued/$5/input-manifest.tsv" && test ! -e "$1/queued/$5/hvf_gic_boot_probe" && grep -Eq '"'"'^input_manifest_sha256=[0-9a-f]{64}$'"'"' "$1/queued/$5/job.env"' _ "$queue_root" "$ROOT/scripts/live-gates/bridgevm-live" "$COMMIT" "$noresult_manifest" "$queued_id"
check "worker publication failure changes the job status to failure" grep -Fq 'status=1' "$ROOT/scripts/live-gates/bridgevm-live-worker.sh"
echo "PASS: Windows product E2E live-tier contracts ($checks checks)"
