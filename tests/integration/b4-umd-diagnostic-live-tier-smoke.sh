#!/usr/bin/env bash
# Deterministic policy tests for the sealed, diagnostic-only B4 lane.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
CLI="$ROOT/scripts/live-gates/bridgevm-live"; PACKAGE="$ROOT/scripts/live-gates/b4-diagnostic-package.py"; ANALYZER="$ROOT/scripts/live-gates/analyze-b4-umd-diagnostic.py"
TIER="$ROOT/scripts/live-gates/run-b4-umd-diagnostic-tier.sh"; VERIFY="$ROOT/scripts/live-gates/verify-live-sealed-input.sh"; INJECT="$ROOT/scripts/live-gates/prepare-b4-offline-injection.sh"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT; export BRIDGEVM_LIVE_ROOT="$WORK/queue" BRIDGEVM_LIVE_MEDIA_CACHE="$WORK/media-cache"; injector_vars="$WORK/injector-vars.fd"; injector_iso="$WORK/windows.iso"; printf vars >"$injector_vars"; printf iso >"$injector_iso"
fail() { echo "FAIL: $*" >&2; exit 1; }
for helper in "$PACKAGE" "$ANALYZER" "$TIER" "$VERIFY" "$INJECT"; do [[ -x "$helper" ]] || { echo "FAIL: non-executable $helper" >&2; exit 1; }; done
"$PACKAGE" self-test | grep -q PASS
"$ANALYZER" --self-test | grep -q PASS
bash "$ROOT/scripts/live-gates/pointer-prepared-cache.sh" | grep -q PASS
python3 - "$ROOT/scripts/win-assets/b4-dbwin-capture.ps1" "$ROOT/scripts/win-assets/b4-install-diagnostic-package.ps1" <<'PY'
import pathlib, sys
for name in sys.argv[1:]:
    data = pathlib.Path(name).read_bytes()
    assert b"\n" in data and data.replace(b"\r\n", b"").find(b"\n") == -1
PY
# shellcheck disable=SC2016
pwsh -NoProfile -Command '$ErrorActionPreference="Stop"; foreach ($p in @("scripts/win-assets/b4-dbwin-capture.ps1", "scripts/win-assets/b4-install-diagnostic-package.ps1")) { $e=$null; [void][Management.Automation.Language.Parser]::ParseFile((Resolve-Path $p),[ref]$null,[ref]$e); if ($e.Count) { throw ($e | Out-String) } }'
grep -Fq "if (\$computedTree -cne \$packageFields[1]) { Fail 'package tree hash mismatch' }" scripts/win-assets/b4-install-diagnostic-package.ps1; grep -Fq "[string]\$PackageRoot = 'C:\BridgeVM\B4DiagPackage'" scripts/win-assets/b4-install-diagnostic-package.ps1; grep -Fq -- '-VerifyOnly -PackageRoot C:\\BridgeVM\\viogpu3d' scripts/run-b4-umd-diagnostic-case.sh; grep -Fq "find \"\$CASE/share\" -type f -name 'b4pkg-*.bin' -delete" scripts/run-b4-umd-diagnostic-case.sh; ! grep -Eq 'B4INSTALL_LAUNCHED|wait_for.*b4pkg-|shutdown /r' scripts/run-b4-umd-diagnostic-case.sh; grep -Fq 'ISO="$INJECTOR_ISO" NETKVM_DIR="$STAGE/no-unsealed-netkvm"' "$INJECT"; grep -Fq '[[ "$observed" == true ]]' "$INJECT"; grep -Fq 'cp "$SOURCE_VARS" "$STAGE/vars.fd"' "$INJECT"; grep -Fq 'prepared_commit' scripts/live-gates/pointer-prepared-cache.sh
package_dir="$WORK/package"; mkdir "$package_dir"
for name in BridgeVM-viogpu3d-Test.cer bridgevm-package-provenance.env viogpu3d.cat viogpu3d.inf viogpu3d.sys viogpu_d3d10.dll virtio_icd.arm64.json vulkan_virtio.dll; do printf '%s\n' "$name" >"$package_dir/$name"; done
printf 'DriverVer= 08/25/2026, 120.50.0.0\n' >>"$package_dir/viogpu3d.inf"; printf 'VIOGPU3D_SOURCE_REF=d780b2b7f76301ef50282be973e95dbe6bba783f + mesa@cb531c440ff34a9c6334859dda0848132be49ec3 + builder@2f74d3332e50a71cf64bc25ee428fc0803334f81:submit-trace+resident-kmd\n' >>"$package_dir/bridgevm-package-provenance.env"; printf 'BV-VIRGL-ALLOC-LIST-GROW-FAIL x\0BV-VIRGL-SUBMIT stage=x\n' >>"$package_dir/viogpu_d3d10.dll"
manifest="$WORK/input.tsv"; tree_hash="$($PACKAGE write-manifest --dir "$package_dir" --out "$manifest")"; vars_hash="$(shasum -a 256 "$injector_vars" | cut -d' ' -f1)"; iso_hash="$(shasum -a 256 "$injector_iso" | cut -d' ' -f1)"
B4_TREE_EXPECTED="$tree_hash" B4_TREE_MANIFEST="$manifest" pwsh -NoProfile -Command '$rows=Get-Content -LiteralPath $env:B4_TREE_MANIFEST; $text=New-Object Text.StringBuilder; foreach($row in $rows){$f=@($row -split "`t"); [void]$text.Append($f[0]).Append([char]0).Append($f[2]).Append("`n")}; $sha=[Security.Cryptography.SHA256]::Create(); try{$actual=([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::ASCII.GetBytes($text.ToString())))).Replace("-","").ToLowerInvariant()}finally{$sha.Dispose()}; if($actual -cne $env:B4_TREE_EXPECTED){throw "PowerShell package tree hash mismatch"}'
for tier in t8-pointer-reliability t12-b4-umd-diagnostic; do if "$CLI" submit "$tier" >/dev/null 2>&1; then fail "$tier accepted no manifest"; fi; done
if "$CLI" submit t0-check --input-manifest "$manifest" >/dev/null 2>&1; then fail 'T0 accepted a manifest'; fi; if "$CLI" submit t12-b4-umd-diagnostic --input-manifest "$manifest" >/dev/null 2>&1; then fail 'T12 accepted missing injector media'; fi
job="$($CLI submit t12-b4-umd-diagnostic --input-manifest "$manifest" --injector-vars "$injector_vars" --injector-iso "$injector_iso")"; dir="$BRIDGEVM_LIVE_ROOT/queued/$job"
t8_job="$($CLI submit t8-pointer-reliability --input-manifest "$manifest" --injector-vars "$injector_vars" --injector-iso "$injector_iso")"; t8_dir="$BRIDGEVM_LIVE_ROOT/queued/$t8_job"
grep -Fqx "sealed_package_sha256=$tree_hash" "$dir/job.env"; grep -Fqx "sealed_injector_vars_sha256=$vars_hash" "$dir/job.env"; grep -Fqx "sealed_injector_iso_sha256=$iso_hash" "$dir/job.env"
grep -Fqx "sealed_package_sha256=$tree_hash" "$t8_dir/job.env"; [[ -f "$dir/input-manifest.tsv" && -d "$dir/sealed-package" && -f "$dir/sealed-injector-vars.fd" ]]
"$VERIFY" t12-b4-umd-diagnostic "$dir" "$ROOT"; "$VERIFY" t8-pointer-reliability "$t8_dir" "$ROOT"; chmod 600 "$dir/sealed-injector-vars.fd"; printf mutation >>"$dir/sealed-injector-vars.fd"; if "$VERIFY" t12-b4-umd-diagnostic "$dir" "$ROOT"; then fail 'mutated injector vars verified'; fi; cp "$injector_vars" "$dir/sealed-injector-vars.fd"; chmod 400 "$dir/sealed-injector-vars.fd"
iso_cache="$(awk -F= '$1=="sealed_injector_iso_path"{print substr($0,index($0,"=")+1)}' "$dir/job.env")"; chmod 600 "$iso_cache"; printf mutation >>"$iso_cache"; if "$VERIFY" t8-pointer-reliability "$t8_dir" "$ROOT"; then fail 'mutated injector ISO verified'; fi; cp "$injector_iso" "$iso_cache"; chmod 400 "$iso_cache"
python3 scripts/live-gates/write-pointer-reliability-receipt.py --out "$WORK/pointer-receipt" --job-id smoke --commit "$tree_hash" --image "$tree_hash" --vars "$tree_hash" --manifest "$tree_hash" --package "$tree_hash" --umd "$tree_hash" --landed 20/20 --p95 250 --outcome completed --pass
python3 scripts/live-gates/redact-receipt.py --in "$WORK/pointer-receipt/receipt.json" --out "$WORK/pointer-public.json"
python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r["pass"] is True and r["sealed_package_sha256"]==sys.argv[2]' "$WORK/pointer-public.json" "$tree_hash"
mv "$dir/input-manifest.tsv" "$dir/input-manifest.real"; ln -s input-manifest.real "$dir/input-manifest.tsv"
if "$VERIFY" t12-b4-umd-diagnostic "$dir" "$ROOT"; then fail 'symlinked queue manifest verified'; fi
unlink "$dir/input-manifest.tsv"; mv "$dir/input-manifest.real" "$dir/input-manifest.tsv"
printf mutation >>"$package_dir/viogpu3d.inf"; printf mutation >>"$manifest"
"$VERIFY" t12-b4-umd-diagnostic "$dir" "$ROOT"; "$VERIFY" t8-pointer-reliability "$t8_dir" "$ROOT"
out="$WORK/refusal"
if "$TIER" --out "$out" --job-id smoke --input-manifest "$dir/input-manifest.tsv" \
  --sealed-package "$dir/sealed-package" --sealed-injector-vars "$dir/sealed-injector-vars.fd" --sealed-injector-iso "$iso_cache" >"$WORK/refusal.log" 2>&1; then fail 'fake package reached a live case'; fi
[[ -f "$out/receipt.json" ]] || { cat "$WORK/refusal.log" >&2; fail 'refusal receipt absent'; }
python3 - "$out/receipt.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))
assert r["tier"] == "t12-b4-umd-diagnostic" and r["criterion"] == "B4"
assert r["sample_count"] == 0 and r["pass"] is False
PY
python3 "$ROOT/scripts/live-gates/redact-receipt.py" --in "$out/receipt.json" --out "$WORK/public.json"
chmod 600 "$dir/sealed-package/viogpu3d.inf"; printf mutation >>"$dir/sealed-package/viogpu3d.inf"; if "$VERIFY" t12-b4-umd-diagnostic "$dir" "$ROOT"; then fail 'mutated queue package verified'; fi
chmod 600 "$t8_dir/sealed-package/viogpu3d.inf"; printf mutation >>"$t8_dir/sealed-package/viogpu3d.inf"; if "$VERIFY" t8-pointer-reliability "$t8_dir" "$ROOT"; then fail 'mutated T8 queue package verified'; fi
for path in "$TIER" "$VERIFY" "$INJECT" "$ROOT/scripts/run-b4-umd-diagnostic-case.sh"; do
  if sed 's/#.*$//' "$path" | grep -Eq 'nc +-l|socat|LISTEN|bind\(|^[^#]*\bsudo\b'; then fail "unsafe local worker path: $path"; fi
done
echo 'PASS: B4 UMD diagnostic live tier policy'
