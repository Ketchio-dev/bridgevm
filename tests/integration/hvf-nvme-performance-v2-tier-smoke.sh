#!/usr/bin/env bash
# Deterministic T16 v2 contracts only; never boots a VM or measures host storage.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd -P)"

python3 "$REPO/scripts/render-hvf-nvme-workload-v2.py" --self-test | grep PASS >/dev/null
"$REPO/scripts/live-gates/hvf-nvme-performance-v2-manifest.sh" self-test | grep PASS >/dev/null
"$REPO/scripts/submit-hvf-nvme-calibration-v2-campaign.sh" --self-test | grep PASS >/dev/null
"$REPO/scripts/live-gates/hvf-nvme-performance-v2-environment.sh" self-test | grep PASS >/dev/null
python3 "$REPO/scripts/verify-hvf-nvme-quiescence-v2.py" --self-test | grep PASS >/dev/null
python3 "$REPO/scripts/write-hvf-nvme-performance-v2-receipt.py" --self-test | grep PASS >/dev/null
python3 "$REPO/scripts/hvf_nvme_performance_v2_report.py" --self-test | grep PASS >/dev/null
bash -n "$REPO/scripts/live-gates/run-hvf-nvme-performance-v2-tier.sh"

python3 - "$REPO" <<'PY'
import importlib.util
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, root / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

renderer = load("bridgevm_v2_renderer_smoke", "scripts/render-hvf-nvme-workload-v2.py")
writer = load("bridgevm_v2_writer_smoke", "scripts/write-hvf-nvme-performance-v2-receipt.py")
assert renderer.config_sha256() == writer.CONFIG_SHA256
assert renderer.PRECONDITION_SHA256 == writer.PRECONDITION_SHA256
assert renderer.FINAL_SHA256 == writer.FINAL_SHA256
asset = (root / "scripts/win-assets/bv-nvme-quiescence-v2.ps1").read_bytes()
assert asset.endswith(b"\r\n") and asset.count(b"\n") == asset.count(b"\r\n")
PY

grep -Fq 'verify_caffeinated_ancestor' "$REPO/scripts/live-gates/run-hvf-nvme-performance-v2-tier.sh"
grep -Fq '$(seal "$INPUT_MANIFEST")' "$REPO/scripts/live-gates/run-hvf-nvme-performance-v2-tier.sh"
grep -Fq 'optional_stopping\tforbidden' "$REPO/scripts/submit-hvf-nvme-calibration-v2-campaign.sh"
grep -Fq 'render-hvf-nvme-workload-v2.py' "$REPO/.github/workflows/ci.yml"
echo "PASS: sealed T16 Windows warm NVMe v1 and v2 performance contracts"
