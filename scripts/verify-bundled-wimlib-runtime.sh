#!/usr/bin/env bash
set -euo pipefail

APP="${1:-}"
VERSION="1.14.5"
SOURCE_SHA256="84221a3abd5b91228f15f8e6065c335a336237b5738197b75bf419eea561a194"
[[ -n "$APP" && -d "$APP/Contents" ]] || {
  echo "usage: scripts/verify-bundled-wimlib-runtime.sh APP" >&2
  exit 2
}

resources="$APP/Contents/Resources"
helper="$resources/helpers/wimlib-imagex"
source_archive="$resources/licenses/wimlib/source/wimlib-${VERSION}.tar.gz"
license_dir="$resources/licenses/wimlib"
build_receipt="$license_dir/build-receipt.json"
package_receipt="$license_dir/package-receipt.json"

for file in "$helper" "$source_archive" "$license_dir/COPYING" \
  "$license_dir/COPYING.GPLv3" "$license_dir/build-pinned-wimlib-runtime.sh" \
  "$build_receipt" "$package_receipt"; do
  [[ -f "$file" && ! -L "$file" ]] || { echo "bundled wimlib file is missing or unsafe: $file" >&2; exit 1; }
done
[[ -x "$helper" ]] || { echo "bundled wimlib helper is not executable" >&2; exit 1; }
[[ "$(/usr/bin/shasum -a 256 "$source_archive" | /usr/bin/awk '{print $1}')" == "$SOURCE_SHA256" ]] || {
  echo "bundled wimlib corresponding source digest mismatch" >&2
  exit 1
}
"$helper" --version | grep -qF "wimlib-imagex $VERSION" || {
  echo "bundled wimlib version mismatch" >&2
  exit 1
}
while IFS= read -r dependency; do
  case "$dependency" in
    /usr/lib/*|/System/Library/*) ;;
    *) echo "bundled wimlib has a non-system dependency: $dependency" >&2; exit 1 ;;
  esac
done < <(/usr/bin/otool -L "$helper" | /usr/bin/awk 'NR > 1 {print $1}')

/usr/bin/python3 - "$build_receipt" "$package_receipt" "$helper" "$SOURCE_SHA256" <<'PY'
import hashlib
import json
import pathlib
import sys

build_path = pathlib.Path(sys.argv[1])
package_path = pathlib.Path(sys.argv[2])
helper_path = pathlib.Path(sys.argv[3])
expected_source = sys.argv[4]
build = json.loads(build_path.read_text())
package = json.loads(package_path.read_text())
expected_build = {
    "binary_sha256_before_codesign", "configure_arguments", "license",
    "source_sha256", "source_url", "version",
}
expected_package = {"binary_sha256", "codesign_class", "source_sha256", "version"}
if set(build) != expected_build or set(package) != expected_package:
    raise SystemExit("wimlib receipt keys do not match the sealed contract")
if build["license"] != "GPL-3.0-or-later" or build["version"] != "1.14.5":
    raise SystemExit("wimlib build identity mismatch")
if build["source_sha256"] != expected_source or package["source_sha256"] != expected_source:
    raise SystemExit("wimlib source identity mismatch")
if package["version"] != build["version"]:
    raise SystemExit("wimlib package/build version mismatch")
actual = hashlib.sha256(helper_path.read_bytes()).hexdigest()
if package["binary_sha256"] != actual:
    raise SystemExit("wimlib signed binary digest mismatch")
if package["codesign_class"] not in {"adhoc", "developer-id"}:
    raise SystemExit("wimlib codesign class is invalid")
PY

/usr/bin/tar -xOf "$source_archive" "wimlib-${VERSION}/COPYING" | cmp -s - "$license_dir/COPYING" || {
  echo "bundled wimlib COPYING differs from corresponding source" >&2
  exit 1
}
/usr/bin/tar -xOf "$source_archive" "wimlib-${VERSION}/COPYING.GPLv3" | cmp -s - "$license_dir/COPYING.GPLv3" || {
  echo "bundled wimlib GPL text differs from corresponding source" >&2
  exit 1
}
codesign --verify --strict "$helper"
echo "bundled_wimlib_runtime=pass"
