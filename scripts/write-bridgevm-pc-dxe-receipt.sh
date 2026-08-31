#!/usr/bin/env bash
# Write the reproducible build receipt after every embedded digest has passed.
set -euo pipefail
if [[ $# -ne 15 ]]; then
  echo "usage: $0 OUTPUT REPO EDK2 GCC LD VECTOR_SIZE VECTOR_SHA CORE_SHA RUNTIME_SHA VARIABLE_SHA PLATFORM_SHA PROBE_SHA FV_SHA FD_SHA FD_SIZE" >&2
  exit 64
fi
output_dir="$1" repo_root="$2" edk2_commit="$3" gcc_version="$4" ld_version="$5"
vector_size="$6" vector_sha="$7" core_sha="$8" runtime_sha="$9" variable_sha="${10}"
platform_sha="${11}" probe_sha="${12}" fv_sha="${13}" artifact_sha="${14}" artifact_size="${15}"
source_tree_sha256="$(python3 - "$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg" <<'PY'
import hashlib
import pathlib
import sys
root = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    name = path.relative_to(root).as_posix().encode()
    data = path.read_bytes()
    digest.update(len(name).to_bytes(4, "big") + name)
    digest.update(len(data).to_bytes(8, "big") + data)
print(digest.hexdigest())
PY
)"
receipt="$output_dir/BridgeVmPcDxeEntry.build.json"
printf '%s\n' '{' '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-reset-to-runtime-variable-platform-table-pcie-ecam-probe",' \
  "  \"edk2Commit\": \"${edk2_commit}\"," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"sourceTreeSha256\": \"${source_tree_sha256}\"," \
  "  \"resetVectorSize\": ${vector_size}," \
  "  \"resetVectorSha256\": \"${vector_sha}\"," \
  "  \"rebasedDxeCoreSha256\": \"${core_sha}\"," \
  "  \"runtimeDxeSha256\": \"${runtime_sha}\"," \
  "  \"variableRuntimeDxeSha256\": \"${variable_sha}\"," \
  "  \"platformTablesDxeSha256\": \"${platform_sha}\"," \
  "  \"dxeProbeSha256\": \"${probe_sha}\"," \
  "  \"firmwareVolumeSha256\": \"${fv_sha}\"," \
  "  \"size\": ${artifact_size}," \
  "  \"sha256\": \"${artifact_sha}\"," \
  '  "claimBoundary": "bounded same-boot variable services, Runtime Architectural Protocol, ACPI/SMBIOS publication, and eight direct PCIe ECAM identity reads; standard PCI enumeration, BAR MMIO, DMA, interrupts, boot manager, and Windows boot remain unproven"' \
  '}' >"$receipt"
echo "$receipt"
