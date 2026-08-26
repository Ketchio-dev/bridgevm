#!/usr/bin/env bash
# Deterministic sealing/policy checks for diagnostic-only t13.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
CLI="$ROOT/scripts/live-gates/bridgevm-live"; VERIFY="$ROOT/scripts/live-gates/verify-live-sealed-input.sh"
INPUT_TOOL="$ROOT/scripts/live-gates/compatibility-observation-input.py"
PACKAGE_TOOL="$ROOT/scripts/live-gates/b4-diagnostic-package.py"
WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT; export BRIDGEVM_LIVE_ROOT="$WORK/queue"
fail(){ echo "FAIL: $*" >&2; exit 1; }
"$INPUT_TOOL" self-test | grep -q PASS
python3 scripts/live-gates/validate-compatibility-observation.py --self-test | grep -q PASS
for path in scripts/win-assets/bvgpu-compatibility-observe.ps1 scripts/win-assets/bvgpu-frametime-series.ps1; do
  python3 - "$path" <<'PY'
import pathlib,sys
d=pathlib.Path(sys.argv[1]).read_bytes(); assert d.count(b'\r\n') and d.count(b'\n')==d.count(b'\r\n')
PY
done
# shellcheck disable=SC2016
pwsh -NoProfile -Command '$e=$null;$k=$null; foreach($p in @("scripts/win-assets/bvgpu-compatibility-observe.ps1","scripts/win-assets/bvgpu-frametime-series.ps1")){[void][Management.Automation.Language.Parser]::ParseFile((Resolve-Path $p),[ref]$k,[ref]$e);if($e.Count){throw($e|Out-String)}}'
grep -Fq 'Invoke-CimMethod -ClassName Win32_Process -MethodName Create' scripts/win-assets/bvgpu-compatibility-observe.ps1
grep -Fq "Join-Path \$env:TEMP 'BridgeVM-Frametime'" scripts/win-assets/bvgpu-frametime-series.ps1
grep -Fq 'rm -rf "$WORK"' scripts/run-windows-compatibility-observation.sh

package="$WORK/package"; mkdir "$package"
for name in BridgeVM-viogpu3d-Test.cer bridgevm-package-provenance.env viogpu3d.cat viogpu3d.inf viogpu3d.sys viogpu_d3d10.dll virtio_icd.arm64.json vulkan_virtio.dll; do printf '%s\n' "$name" > "$package/$name"; done
printf 'DriverVer=08/25/2026,120.50.0.0\n' >> "$package/viogpu3d.inf"
printf 'VIOGPU3D_SOURCE_REF=d780b2b7f76301ef50282be973e95dbe6bba783f + mesa@cb531c440ff34a9c6334859dda0848132be49ec3 + builder@2f74d3332e50a71cf64bc25ee428fc0803334f81:submit-trace+resident-kmd\n' >> "$package/bridgevm-package-provenance.env"
printf 'BV-VIRGL-ALLOC-LIST-GROW-FAIL x\0BV-VIRGL-SUBMIT stage=x\n' >> "$package/viogpu_d3d10.dll"
b4_manifest="$WORK/b4.tsv"; tree_hash="$($PACKAGE_TOOL write-manifest --dir "$package" --out "$b4_manifest")"
image="$WORK/image.bin"; vars="$WORK/vars.bin"; printf image > "$image"; printf vars > "$vars"; chmod 400 "$image" "$vars"
candidates="$WORK/candidates.tsv"
printf 'id\tpackage\tapp_id\texecutable\texecutable_sha256\tblockmap_sha256\tversion\tstatic_graphics_imports\n' > "$candidates"
for n in $(seq -w 0 19); do printf 'real-app-%s\tPublisher.App_%s_arm64__pub\tApp\tApp.exe\t%s\t%s\t1.0\tnone-static\n' "$n" "$n" "$(printf a%.0s {1..64})" "$(printf b%.0s {1..64})" >> "$candidates"; done
meta="$WORK/meta.tsv"
prepared="$WORK/$(shasum -a 256 "$image"|cut -d' ' -f1)-$(shasum -a 256 "$vars"|cut -d' ' -f1)"; mkdir "$prepared"
mv "$image" "$prepared/disk.raw"; mv "$vars" "$prepared/vars.fd"; image="$prepared/disk.raw"; vars="$prepared/vars.fd"
"$INPUT_TOOL" write-manifest --image "$image" --vars "$vars" --candidates "$candidates" --b4-manifest "$b4_manifest" --out "$meta" >/dev/null
if "$CLI" submit t13-compatibility-observation >/dev/null 2>&1; then fail 't13 accepted no manifest'; fi
ln -s "$meta" "$WORK/meta-link.tsv"
if "$CLI" submit t13-compatibility-observation --input-manifest "$WORK/meta-link.tsv" >/dev/null 2>&1; then fail 't13 accepted symlink manifest'; fi
job="$($CLI submit t13-compatibility-observation --input-manifest "$meta")"; dir="$BRIDGEVM_LIVE_ROOT/queued/$job"
grep -Fqx "sealed_package_sha256=$tree_hash" "$dir/job.env"
[[ -f "$dir/sealed-compatibility/sealed-candidates.tsv" && -f "$dir/sealed-compatibility/b4-input-manifest.tsv" ]]
"$VERIFY" t13-compatibility-observation "$dir" "$ROOT"
printf mutation >> "$candidates"; "$VERIFY" t13-compatibility-observation "$dir" "$ROOT"
chmod 600 "$image"; printf mutation >> "$image"
if "$VERIFY" t13-compatibility-observation "$dir" "$ROOT"; then fail 'mutated source image verified'; fi
printf image > "$image"; chmod 400 "$image"; "$VERIFY" t13-compatibility-observation "$dir" "$ROOT"
chmod 600 "$dir/sealed-compatibility/sealed-candidates.tsv"; printf mutation >> "$dir/sealed-compatibility/sealed-candidates.tsv"
if "$VERIFY" t13-compatibility-observation "$dir" "$ROOT"; then fail 'mutated sealed candidates verified'; fi
python3 scripts/live-gates/redact-receipt.py --self-test | grep -q PASS
for path in scripts/live-gates/run-compatibility-observation-tier.sh scripts/run-windows-compatibility-observation.sh; do
  if sed 's/#.*$//' "$path" | grep -Eq 'nc +-l|socat|LISTEN|bind\(|^[^#]*\bsudo\b'; then fail "unsafe compatibility worker path: $path"; fi
done
echo 'PASS: compatibility observation live tier policy'
