#!/usr/bin/env bash
# Build the separately executed wimlib-imagex helper and retain complete source.
set -euo pipefail

VERSION="1.14.5"
SOURCE_URL="https://wimlib.net/downloads/wimlib-${VERSION}.tar.gz"
SOURCE_SHA256="84221a3abd5b91228f15f8e6065c335a336237b5738197b75bf419eea561a194"
OUTPUT=""
SOURCE_ARCHIVE=""

usage() {
  echo "usage: scripts/build-pinned-wimlib-runtime.sh --output DIR [--source-archive TAR_GZ]" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) [[ $# -ge 2 ]] || { usage; exit 2; }; OUTPUT="$2"; shift 2 ;;
    --source-archive) [[ $# -ge 2 ]] || { usage; exit 2; }; SOURCE_ARCHIVE="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done

[[ -n "$OUTPUT" ]] || { usage; exit 2; }
[[ "$OUTPUT" == /* ]] || { echo "output must be absolute: $OUTPUT" >&2; exit 2; }
[[ ! -e "$OUTPUT" ]] || { echo "refusing to overwrite output: $OUTPUT" >&2; exit 1; }
output_parent="$(cd "$(dirname "$OUTPUT")" && pwd -P)"
output_name="$(basename "$OUTPUT")"
[[ "$output_name" != "." && "$output_name" != ".." && -n "$output_name" ]] || {
  echo "unsafe output name: $OUTPUT" >&2
  exit 2
}

work="$(mktemp -d "$output_parent/.bridgevm-wimlib-build.XXXXXX")"
stage="$work/$output_name"
cleanup() { rm -rf "$work"; }
trap cleanup EXIT
mkdir -p "$work/tmp" "$stage/helpers" "$stage/licenses/wimlib" "$stage/source"

archive="$work/wimlib-${VERSION}.tar.gz"
if [[ -n "$SOURCE_ARCHIVE" ]]; then
  [[ "$SOURCE_ARCHIVE" == /* && -f "$SOURCE_ARCHIVE" && ! -L "$SOURCE_ARCHIVE" ]] || {
    echo "source archive must be an absolute regular non-symlink file" >&2
    exit 2
  }
  /bin/cp "$SOURCE_ARCHIVE" "$archive"
else
  /usr/bin/curl --fail --silent --show-error --location "$SOURCE_URL" --output "$archive"
fi
actual_source_sha="$(/usr/bin/shasum -a 256 "$archive" | /usr/bin/awk '{print $1}')"
[[ "$actual_source_sha" == "$SOURCE_SHA256" ]] || {
  echo "wimlib source digest mismatch: $actual_source_sha" >&2
  exit 1
}

/usr/bin/tar -xzf "$archive" -C "$work"
source_root="$work/wimlib-${VERSION}"
for required in configure COPYING COPYING.GPLv3; do
  [[ -f "$source_root/$required" && ! -L "$source_root/$required" ]] || {
    echo "wimlib source is missing $required" >&2
    exit 1
  }
done
pkg_config=""
for candidate in /opt/homebrew/bin/pkg-config /usr/local/bin/pkg-config; do
  if [[ -x "$candidate" ]]; then
    resolved="$(/usr/bin/python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$candidate")"
    case "$resolved" in
      /opt/homebrew/Cellar/pkgconf/*/bin/pkgconf|/usr/local/Cellar/pkgconf/*/bin/pkgconf)
        [[ -f "$resolved" && ! -L "$resolved" ]] || continue
        pkg_config="$resolved"
        break
        ;;
    esac
  fi
done
[[ -n "$pkg_config" ]] || {
  echo "pkg-config is required at a fixed Homebrew prefix to build wimlib" >&2
  exit 1
}

(
  cd "$source_root"
  /usr/bin/env -i \
    PATH=/usr/bin:/bin:/usr/sbin:/sbin \
    TMPDIR="$work/tmp" LC_ALL=C \
    CC=/usr/bin/clang AR=/usr/bin/ar RANLIB=/usr/bin/ranlib STRIP=/usr/bin/strip \
    PKG_CONFIG="$pkg_config" \
    ./configure --without-ntfs-3g --without-fuse --disable-shared --enable-static
  /usr/bin/env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin TMPDIR="$work/tmp" LC_ALL=C \
    /usr/bin/make -j4 wimlib-imagex
)

helper="$source_root/wimlib-imagex"
[[ -x "$helper" && ! -L "$helper" ]] || { echo "wimlib-imagex build output is missing" >&2; exit 1; }
version_output="$($helper --version)"
grep -qF "wimlib-imagex $VERSION" <<<"$version_output" || {
  echo "wimlib-imagex reported an unexpected version" >&2
  exit 1
}
while IFS= read -r dependency; do
  case "$dependency" in
    /usr/lib/*|/System/Library/*) ;;
    *) echo "wimlib-imagex has a non-system dependency: $dependency" >&2; exit 1 ;;
  esac
done < <(/usr/bin/otool -L "$helper" | /usr/bin/awk 'NR > 1 {print $1}')

/usr/bin/install -m 755 "$helper" "$stage/helpers/wimlib-imagex"
/usr/bin/install -m 644 "$archive" "$stage/source/wimlib-${VERSION}.tar.gz"
/usr/bin/install -m 644 "$source_root/COPYING" "$source_root/COPYING.GPLv3" \
  "$stage/licenses/wimlib/"
/usr/bin/install -m 644 "$0" "$stage/licenses/wimlib/build-pinned-wimlib-runtime.sh"

unsigned_sha="$(/usr/bin/shasum -a 256 "$stage/helpers/wimlib-imagex" | /usr/bin/awk '{print $1}')"
/usr/bin/python3 - "$stage/licenses/wimlib/build-receipt.json" \
  "$VERSION" "$SOURCE_URL" "$SOURCE_SHA256" "$unsigned_sha" <<'PY'
import json
import pathlib
import sys

path, version, url, source_sha, binary_sha = sys.argv[1:]
receipt = {
    "binary_sha256_before_codesign": binary_sha,
    "configure_arguments": [
        "--without-ntfs-3g", "--without-fuse", "--disable-shared", "--enable-static"
    ],
    "license": "GPL-3.0-or-later",
    "source_sha256": source_sha,
    "source_url": url,
    "version": version,
}
pathlib.Path(path).write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
PY

mv "$stage" "$OUTPUT"
trap - EXIT
rm -rf "$work"
printf '%s\n' "$OUTPUT"
