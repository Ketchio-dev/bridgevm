#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 --runtime DIR --app APP [--identity IDENTITY]" >&2
}

RUNTIME=""
APP=""
IDENTITY="-"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --runtime) [[ $# -ge 2 ]] || { usage; exit 2; }; RUNTIME="$2"; shift 2 ;;
    --app) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; shift 2 ;;
    --identity) [[ $# -ge 2 ]] || { usage; exit 2; }; IDENTITY="$2"; shift 2 ;;
    *) usage; exit 2 ;;
  esac
done
[[ -n "$RUNTIME" && -d "$RUNTIME" && ! -L "$RUNTIME" ]] || {
  echo "--runtime must name a build-pinned-wimlib-runtime.sh output directory" >&2
  exit 2
}
[[ -n "$APP" && -d "$APP/Contents/Resources" && ! -L "$APP" ]] || {
  echo "--app must name the package staging app" >&2
  exit 2
}

required=(
  helpers/wimlib-imagex
  licenses/wimlib/COPYING
  licenses/wimlib/COPYING.GPLv3
  licenses/wimlib/build-pinned-wimlib-runtime.sh
  licenses/wimlib/build-receipt.json
  source/wimlib-1.14.5.tar.gz
)
for path in "${required[@]}"; do
  [[ -f "$RUNTIME/$path" && ! -L "$RUNTIME/$path" ]] || {
    echo "wimlib runtime is missing a required regular file: $path" >&2
    exit 1
  }
done

license_dir="$APP/Contents/Resources/licenses/wimlib"
install -d "$APP/Contents/Resources/helpers" "$license_dir/source"
install -m 755 "$RUNTIME/helpers/wimlib-imagex" "$APP/Contents/Resources/helpers/wimlib-imagex"
install -m 644 \
  "$RUNTIME/licenses/wimlib/COPYING" \
  "$RUNTIME/licenses/wimlib/COPYING.GPLv3" \
  "$RUNTIME/licenses/wimlib/build-pinned-wimlib-runtime.sh" \
  "$RUNTIME/licenses/wimlib/build-receipt.json" \
  "$license_dir/"
install -m 644 "$RUNTIME/source/wimlib-1.14.5.tar.gz" "$license_dir/source/wimlib-1.14.5.tar.gz"

helper="$APP/Contents/Resources/helpers/wimlib-imagex"
if [[ "$IDENTITY" == "-" ]]; then
  codesign --force --sign - "$helper" >/dev/null
  codesign_class="adhoc"
else
  codesign --force --sign "$IDENTITY" --options runtime --timestamp "$helper" >/dev/null
  codesign_class="developer-id"
fi
binary_sha="$(shasum -a 256 "$helper" | awk '{print $1}')"
source_sha="$(shasum -a 256 "$license_dir/source/wimlib-1.14.5.tar.gz" | awk '{print $1}')"
/usr/bin/python3 - "$license_dir/package-receipt.json" "$binary_sha" "$source_sha" "$codesign_class" <<'PY'
import json
import pathlib
import sys

path, binary_sha, source_sha, codesign_class = sys.argv[1:]
receipt = {
    "binary_sha256": binary_sha,
    "codesign_class": codesign_class,
    "source_sha256": source_sha,
    "version": "1.14.5",
}
pathlib.Path(path).write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
PY
