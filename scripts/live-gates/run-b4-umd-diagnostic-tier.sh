#!/usr/bin/env bash
# t12-b4-umd-diagnostic: one retained correlation lane, never a B4 pass.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT=""; INPUT_MANIFEST=""; SEALED_PACKAGE=""; JOB_ID="local-$(date +%Y%m%d-%H%M%S)"
while (( $# )); do
  case "$1" in
    --out) OUT="$2"; shift 2 ;;
    --input-manifest) INPUT_MANIFEST="$2"; shift 2 ;;
    --sealed-package) SEALED_PACKAGE="$2"; shift 2 ;;
    --job-id) JOB_ID="$2"; shift 2 ;;
    *) echo "unknown B4 diagnostic tier option $1" >&2; exit 2 ;;
  esac
done
[[ -n "$OUT" && -n "$INPUT_MANIFEST" && -n "$SEALED_PACKAGE" ]] || { echo 'B4 diagnostic tier needs --out, --input-manifest, and --sealed-package' >&2; exit 2; }
mkdir -p "$OUT"
PACKAGE_TOOL="$REPO/scripts/live-gates/b4-diagnostic-package.py"
BUILDER_SHA=2f74d3332e50a71cf64bc25ee428fc0803334f81
EXPECTED_VERSION=120.50.0.0
commit="$(git -C "$REPO" rev-parse HEAD)"; image=absent; vars=absent
manifest_hash=absent; package_hash=absent; umd_hash=absent
sample_count=0; outcome=infrastructure-failed; analysis_path="$OUT/analysis.json"
# shellcheck disable=SC2329
write_receipt() {
  python3 - "$OUT" "$JOB_ID" "$commit" "$image" "$vars" "$manifest_hash" \
    "$package_hash" "$umd_hash" "$EXPECTED_VERSION" "$sample_count" "$outcome" "$analysis_path" <<'PY'
import json, platform, subprocess, sys
from pathlib import Path
out = Path(sys.argv[1]); analysis_path = Path(sys.argv[12])
host = subprocess.run(["sysctl", "-n", "hw.model"], text=True, capture_output=True)
host_model = host.stdout.strip() if host.returncode == 0 else "unavailable"
verify = out / "case/share/b4-verify-result.log"
values = {}
if verify.is_file():
    for line in verify.read_text(errors="replace").replace("\r", "").splitlines():
        if "=" in line:
            key, value = line.split("=", 1); values[key] = value
analysis = json.loads(analysis_path.read_text()) if analysis_path.is_file() else {}
guest = analysis.get("first_grow_fail") or {}; host = analysis.get("first_never_backed_transfer") or {}
evidence = [str(path.relative_to(out)) for path in (
    out / "case/share/b4-dbwin.log", out / "case/virtio-gpu.jsonl", analysis_path,
    out / "case/analysis-window.env", out / "case/summary.txt",
) if path.is_file()]
receipt = {
    "tier": "t12-b4-umd-diagnostic", "gate_id": "b4-umd-host-resource-correlation",
    "criterion": "B4", "job_id": sys.argv[2], "commit": sys.argv[3],
    "image_sha256": sys.argv[4], "vars_sha256": sys.argv[5],
    "input_manifest_sha256": sys.argv[6], "sealed_package_sha256": sys.argv[7],
    "diagnostic_umd_sha256": sys.argv[8], "diagnostic_version": sys.argv[9],
    "host_model": host_model,
    "macos_version": platform.mac_ver()[0], "sample_count": int(sys.argv[10]),
    "installed_diagnostic_verified": values.get("verified") == "true",
    "installed_umd_sha256": values.get("installed_umd_sha256", "absent"),
    "driver_version": values.get("driver_version", "absent"),
    "diagnostic_correlation": bool(analysis.get("correlated", False)),
    "diagnostic_guest_event_count": int(analysis.get("dbwin_grow_fail_count", 0)),
    "diagnostic_submit_event_count": int(analysis.get("dbwin_submit_count", 0)),
    "diagnostic_max_submit_allocations": int(analysis.get("max_submit_allocations", 0)),
    "diagnostic_max_submit_capacity": int(analysis.get("max_submit_capacity", 0)),
    "diagnostic_max_d3d_list_size": int(analysis.get("max_d3d_list_size", 0)),
    "diagnostic_host_event_count": int(analysis.get("host_never_backed_transfer_count", 0)),
    "diagnostic_guest_resource_id": guest.get("resource_id", "absent"),
    "diagnostic_host_resource_id": host.get("resource_id", "absent"),
    "evidence_paths": evidence, "outcome": sys.argv[11], "pass": False,
    "known_confounders": ["Diagnostic correlation only; B4 still requires 20/20 landed clicks and p95 <= 250 ms."],
}
(out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
PY
}
# shellcheck disable=SC2329
finish() { rc=$?; trap - EXIT; rm -rf "${OUT:?}/work"; write_receipt || true; exit "$rc"; }
trap finish EXIT
[[ -f "$INPUT_MANIFEST" && -d "$SEALED_PACKAGE" ]] || { echo 'sealed B4 diagnostic inputs are absent' >&2; exit 1; }
manifest_hash="$(openssl dgst -sha256 -r "$INPUT_MANIFEST" | cut -d' ' -f1)"
package_hash="$($PACKAGE_TOOL verify --manifest "$INPUT_MANIFEST" --dir "$SEALED_PACKAGE")"
provenance="$SEALED_PACKAGE/bridgevm-package-provenance.env"
grep -Fqx "VIOGPU3D_SOURCE_REF=d780b2b7f76301ef50282be973e95dbe6bba783f + mesa@cb531c440ff34a9c6334859dda0848132be49ec3 + builder@$BUILDER_SHA:submit-trace+resident-kmd" "$provenance" || { echo 'diagnostic provenance ref mismatch' >&2; exit 1; }
grep -Fq "DriverVer= 08/25/2026, $EXPECTED_VERSION" "$SEALED_PACKAGE/viogpu3d.inf" || { echo 'diagnostic INF version mismatch' >&2; exit 1; }
for marker in 'BV-VIRGL-ALLOC-LIST-GROW-FAIL ' 'BV-VIRGL-SUBMIT stage='; do
  LC_ALL=C grep -aFq "$marker" "$SEALED_PACKAGE/viogpu_d3d10.dll" || { echo "diagnostic UMD marker absent: $marker" >&2; exit 1; }
done
"$REPO/scripts/check-hvf-windows-viogpu3d-package.sh" --require-render-candidate "$SEALED_PACKAGE" >"$OUT/package-check.log"
umd_hash="$(openssl dgst -sha256 -r "$SEALED_PACKAGE/viogpu_d3d10.dll" | cut -d' ' -f1)"
PREPARED=${PREPARED:-$HOME/BridgeVM/prepared/windows-1.0/d7a95823e889db5f4a24948be50653aaec92fb789adc8ff763c27c83be080b16-c61e2136c23b5e0a681f5d33810f617ae6ffc3ea7df0a950248c311767714265}
TARGET="$PREPARED/disk.raw"; VARS="$PREPARED/vars.fd"
for input in "$TARGET" "$VARS"; do [[ -f "$input" && ! -L "$input" ]] && head -c1 "$input" >/dev/null 2>&1 || { echo "cannot read regular diagnostic Windows media: $input" >&2; exit 1; }; done
image="$(openssl dgst -sha256 -r "$TARGET" | cut -d' ' -f1)"; vars="$(openssl dgst -sha256 -r "$VARS" | cut -d' ' -f1)"
[[ "$(basename "$PREPARED")" == "$image-$vars" ]] || { echo 'prepared Windows media identity mismatch' >&2; exit 1; }
case_status=0
RUN="$OUT/case" WORK="$OUT/work" TARGET="$TARGET" VARS="$VARS" VIOGPU_DIR="$SEALED_PACKAGE" INPUT_MANIFEST="$INPUT_MANIFEST" \
  bash "$REPO/scripts/run-b4-umd-diagnostic-case.sh" >"$OUT/gate.log" 2>&1 || case_status=$?
case_analysis="$OUT/case/correlation.json"; analyzer_status=1
if [[ -f "$case_analysis" ]]; then cp "$case_analysis" "$analysis_path" && analyzer_status=0; fi
if (( case_status == 0 && analyzer_status == 0 )); then
  sample_count=1; outcome="diagnostic-$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["outcome"])' "$analysis_path")"
  exit 0
fi
echo "B4 diagnostic lane incomplete: case=$case_status analyzer=$analyzer_status" >&2
exit 1
