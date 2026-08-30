#!/usr/bin/env bash
# Write the reproducible build receipt after every embedded digest has passed.
set -euo pipefail

if [[ $# -ne 14 ]]; then
  echo "usage: $0 OUTPUT REPO EDK2 GCC LD VECTOR_SIZE VECTOR_SHA CORE_SHA RUNTIME_SHA PLATFORM_SHA PROBE_SHA FV_SHA FD_SHA FD_SIZE" >&2
  exit 64
fi
output_dir="$1" repo_root="$2" edk2_commit="$3" gcc_version="$4" ld_version="$5"
vector_size="$6" vector_sha="$7" core_sha="$8" runtime_sha="$9" platform_sha="${10}"
probe_sha="${11}" fv_sha="${12}" artifact_sha="${13}" artifact_size="${14}"
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
  '  "artifactKind": "development-only-reset-to-runtime-platform-table-probe",' \
  "  \"edk2Commit\": \"${edk2_commit}\"," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"sourceTreeSha256\": \"${source_tree_sha256}\"," \
  "  \"resetVectorSize\": ${vector_size}," \
  "  \"resetVectorSha256\": \"${vector_sha}\"," \
  "  \"rebasedDxeCoreSha256\": \"${core_sha}\"," \
  "  \"runtimeDxeSha256\": \"${runtime_sha}\"," \
  "  \"platformTablesDxeSha256\": \"${platform_sha}\"," \
  "  \"dxeProbeSha256\": \"${probe_sha}\"," \
  "  \"firmwareVolumeSha256\": \"${fv_sha}\"," \
  "  \"size\": ${artifact_size}," \
  "  \"sha256\": \"${artifact_sha}\"," \
  '  "claimBoundary": "bounded Runtime Architectural Protocol plus ACPI/SMBIOS publication only; no variables, reset/time services, boot manager, or Windows boot claim"' \
  '}' >"$receipt"
echo "$receipt"
