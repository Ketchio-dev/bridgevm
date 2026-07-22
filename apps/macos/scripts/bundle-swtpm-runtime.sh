#!/usr/bin/env bash
set -euo pipefail

IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
EXPECTED_SWTPM_VERSION="${BRIDGEVM_SWTPM_VERSION:-0.10.1}"
EXPECTED_LIBTPMS_VERSION="${BRIDGEVM_LIBTPMS_VERSION:-0.10.2}"
SWTPM_BIN="${BRIDGEVM_SWTPM_BIN:-}"

usage() {
  cat >&2 <<'EOF'
usage: apps/macos/scripts/bundle-swtpm-runtime.sh --app APP [--swtpm-bin PATH]
       apps/macos/scripts/bundle-swtpm-runtime.sh --verify-only APP

Copies the pinned swtpm runtime and its complete non-system dylib closure into
BridgeVMControl.app, rewrites every development-host install name, signs the
runtime, and writes a version/hash/license manifest.

Environment:
  BRIDGEVM_CODESIGN_IDENTITY  signing identity, defaults to ad-hoc '-'
  BRIDGEVM_SWTPM_BIN          source executable, defaults to command -v swtpm
  BRIDGEVM_SWTPM_VERSION      required version, defaults to 0.10.1
  BRIDGEVM_LIBTPMS_VERSION    required version, defaults to 0.10.2
EOF
}

APP=""
VERIFY_ONLY=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; shift 2 ;;
    --swtpm-bin) [[ $# -ge 2 ]] || { usage; exit 2; }; SWTPM_BIN="$2"; shift 2 ;;
    --verify-only) [[ $# -ge 2 ]] || { usage; exit 2; }; APP="$2"; VERIFY_ONLY=1; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done

[[ -n "$APP" && "$APP" == *.app ]] || { usage; exit 2; }

HELPER="$APP/Contents/Helpers/swtpm"
FRAMEWORKS="$APP/Contents/Frameworks"
RUNTIME_RESOURCES="$APP/Contents/Resources/swtpm"
MANIFEST="$RUNTIME_RESOURCES/manifest.txt"
LICENSES="$RUNTIME_RESOURCES/licenses"

is_system_dependency() {
  case "$1" in
    /usr/lib/*|/System/Library/*) return 0 ;;
    *) return 1 ;;
  esac
}

dependency_paths() {
  otool -L "$1" | awk 'NR > 1 { print $1 }'
}

verify_no_host_paths() {
  local artifact="$1"
  local dependency
  while IFS= read -r dependency; do
    [[ -n "$dependency" ]] || continue
    if is_system_dependency "$dependency"; then
      continue
    fi
    case "$dependency" in
      @executable_path/../Frameworks/*)
        [[ -f "$FRAMEWORKS/${dependency#@executable_path/../Frameworks/}" ]] || {
          echo "swtpm runtime dependency is not bundled: $artifact -> $dependency" >&2
          exit 1
        }
        ;;
      @loader_path/*)
        [[ -f "$(dirname "$artifact")/${dependency#@loader_path/}" ]] || {
          echo "swtpm runtime dependency is not bundled: $artifact -> $dependency" >&2
          exit 1
        }
        ;;
      @rpath/*)
        [[ "$dependency" == "@rpath/$(basename "$artifact")" ]] || {
          echo "swtpm runtime has an unresolved rpath dependency: $artifact -> $dependency" >&2
          exit 1
        }
        ;;
      *)
        echo "swtpm runtime retains an external dependency: $artifact -> $dependency" >&2
        exit 1
        ;;
    esac
  done < <(dependency_paths "$artifact")
  if otool -L "$artifact" | grep -E '/Users/|/opt/homebrew/|/usr/local/' >/dev/null; then
    echo "swtpm runtime retains a development-host path: $artifact" >&2
    exit 1
  fi
}

verify_runtime() {
  [[ -x "$HELPER" && ! -L "$HELPER" ]] || {
    echo "bundled swtpm helper is missing, non-executable, or a symlink: $HELPER" >&2
    exit 1
  }
  [[ -s "$MANIFEST" ]] || { echo "swtpm runtime manifest is missing: $MANIFEST" >&2; exit 1; }
  [[ -d "$LICENSES" ]] || { echo "swtpm runtime license directory is missing: $LICENSES" >&2; exit 1; }
  grep -Fx "format=bridgevm-swtpm-runtime-v1" "$MANIFEST" >/dev/null || {
    echo "swtpm runtime manifest has an unsupported format" >&2
    exit 1
  }
  grep -Fx "swtpm_version=$EXPECTED_SWTPM_VERSION" "$MANIFEST" >/dev/null || {
    echo "swtpm runtime manifest does not pin swtpm $EXPECTED_SWTPM_VERSION" >&2
    exit 1
  }
  grep -Fx "libtpms_version=$EXPECTED_LIBTPMS_VERSION" "$MANIFEST" >/dev/null || {
    echo "swtpm runtime manifest does not pin libtpms $EXPECTED_LIBTPMS_VERSION" >&2
    exit 1
  }
  "$HELPER" --version 2>&1 | grep -F "version $EXPECTED_SWTPM_VERSION" >/dev/null || {
    echo "bundled swtpm does not report version $EXPECTED_SWTPM_VERSION" >&2
    exit 1
  }

  local kind relative expected actual artifact
  while IFS='|' read -r kind relative expected; do
    [[ "$kind" == "artifact" ]] || continue
    artifact="$APP/Contents/$relative"
    [[ -f "$artifact" && ! -L "$artifact" ]] || {
      echo "swtpm manifest artifact is missing or a symlink: $relative" >&2
      exit 1
    }
    actual="$(shasum -a 256 "$artifact" | awk '{ print $1 }')"
    [[ "$expected" =~ ^[0-9a-f]{64}$ && "$actual" == "$expected" ]] || {
      echo "swtpm manifest digest mismatch: $relative" >&2
      exit 1
    }
    codesign --verify --strict "$artifact" >/dev/null 2>&1 || {
      echo "swtpm runtime signature verification failed: $relative" >&2
      exit 1
    }
    verify_no_host_paths "$artifact"
  done < "$MANIFEST"

  local license_count
  license_count="$(find "$LICENSES" -type f | wc -l | tr -d ' ')"
  [[ "$license_count" -ge 2 ]] || {
    echo "swtpm runtime license set is incomplete" >&2
    exit 1
  }
  local component formula version
  while IFS='|' read -r component formula version; do
    [[ "$component" == "component" ]] || continue
    find "$LICENSES" -type f -name "$formula-$version-*" -print -quit | grep -q . || {
      echo "swtpm runtime license is missing for $formula $version" >&2
      exit 1
    }
  done < "$MANIFEST"
}

if [[ "$VERIFY_ONLY" == "1" ]]; then
  verify_runtime
  printf '%s\n' "$APP"
  exit 0
fi

[[ -d "$APP/Contents" ]] || { echo "app bundle Contents directory is missing: $APP" >&2; exit 1; }
if [[ -z "$SWTPM_BIN" ]]; then
  SWTPM_BIN="$(command -v swtpm 2>/dev/null || true)"
fi
[[ -n "$SWTPM_BIN" && -x "$SWTPM_BIN" ]] || {
  echo "swtpm $EXPECTED_SWTPM_VERSION is required to package the Windows HVF runtime" >&2
  exit 1
}
SWTPM_BIN="$(realpath "$SWTPM_BIN")"
"$SWTPM_BIN" --version 2>&1 | grep -F "version $EXPECTED_SWTPM_VERSION" >/dev/null || {
  echo "swtpm source must report version $EXPECTED_SWTPM_VERSION: $SWTPM_BIN" >&2
  exit 1
}

work="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-swtpm-bundle.XXXXXX")"
trap 'rm -rf "$work"' EXIT
pending="$work/pending"
seen="$work/seen"
names="$work/names"
components="$work/components"
: > "$pending"
: > "$seen"
: > "$names"
: > "$components"

enqueue_dependencies() {
  local binary="$1"
  local binary_real dependency resolved
  binary_real="$(realpath "$binary")"
  while IFS= read -r dependency; do
    [[ -n "$dependency" ]] || continue
    is_system_dependency "$dependency" && continue
    case "$dependency" in
      @loader_path/*)
        resolved="$(realpath "$(dirname "$binary_real")/${dependency#@loader_path/}" 2>/dev/null || true)"
        ;;
      @executable_path/*|@rpath/*)
        echo "unsupported source install name while collecting swtpm: $binary -> $dependency" >&2
        exit 1
        ;;
      *) resolved="$(realpath "$dependency" 2>/dev/null || true)" ;;
    esac
    [[ -n "$resolved" && -f "$resolved" ]] || {
      echo "unable to resolve swtpm dependency: $binary -> $dependency" >&2
      exit 1
    }
    [[ "$resolved" == "$binary_real" ]] && continue
    if ! grep -Fx "$resolved" "$seen" "$pending" >/dev/null 2>&1; then
      printf '%s\n' "$resolved" >> "$pending"
    fi
  done < <(dependency_paths "$binary")
}

enqueue_dependencies "$SWTPM_BIN"
while [[ -s "$pending" ]]; do
  current="$(sed -n '1p' "$pending")"
  tail -n +2 "$pending" > "$work/pending.next"
  mv "$work/pending.next" "$pending"
  grep -Fx "$current" "$seen" >/dev/null 2>&1 && continue
  name="$(basename "$current")"
  collision="$(awk -F'|' -v name="$name" '$1 == name { print $2; exit }' "$names")"
  if [[ -n "$collision" && "$collision" != "$current" ]]; then
    echo "swtpm dependency basename collision: $name" >&2
    echo "  $collision" >&2
    echo "  $current" >&2
    exit 1
  fi
  printf '%s|%s\n' "$name" "$current" >> "$names"
  printf '%s\n' "$current" >> "$seen"
  enqueue_dependencies "$current"
done

libtpms_source="$(awk -F'|' '$1 == "libtpms.0.dylib" { print $2; exit }' "$names")"
[[ -n "$libtpms_source" && "$libtpms_source" == */Cellar/libtpms/$EXPECTED_LIBTPMS_VERSION/* ]] || {
  echo "swtpm must resolve to Homebrew libtpms $EXPECTED_LIBTPMS_VERSION" >&2
  exit 1
}

install -d "$APP/Contents/Helpers" "$FRAMEWORKS" "$LICENSES"
install -m 755 "$SWTPM_BIN" "$HELPER"
while IFS='|' read -r name source; do
  install -m 755 "$source" "$FRAMEWORKS/$name"
done < "$names"

rewrite_dependencies() {
  local original="$1"
  local destination="$2"
  local executable_mode="$3"
  local original_real dependency resolved name replacement
  original_real="$(realpath "$original")"
  while IFS= read -r dependency; do
    [[ -n "$dependency" ]] || continue
    is_system_dependency "$dependency" && continue
    case "$dependency" in
      @loader_path/*) resolved="$(realpath "$(dirname "$original_real")/${dependency#@loader_path/}")" ;;
      *) resolved="$(realpath "$dependency")" ;;
    esac
    [[ "$resolved" == "$original_real" ]] && continue
    name="$(basename "$resolved")"
    if [[ "$executable_mode" == "1" ]]; then
      replacement="@executable_path/../Frameworks/$name"
    else
      replacement="@loader_path/$name"
    fi
    install_name_tool -change "$dependency" "$replacement" "$destination"
  done < <(dependency_paths "$original")
}

rewrite_dependencies "$SWTPM_BIN" "$HELPER" 1
while IFS='|' read -r name source; do
  install_name_tool -id "@rpath/$name" "$FRAMEWORKS/$name"
  rewrite_dependencies "$source" "$FRAMEWORKS/$name" 0
done < "$names"

while IFS= read -r source; do
  cellar_line="$(printf '%s\n' "$source" | awk -F/ '$3 == "homebrew" && $4 == "Cellar" { print $5 "|" $6; exit }')"
  [[ -n "$cellar_line" ]] || {
    echo "non-system swtpm dependency is not a versioned Homebrew artifact: $source" >&2
    exit 1
  }
  grep -Fx "$cellar_line" "$components" >/dev/null 2>&1 || printf '%s\n' "$cellar_line" >> "$components"
done < <(printf '%s\n' "$SWTPM_BIN"; cat "$seen")

while IFS='|' read -r formula version; do
  prefix="/opt/homebrew/Cellar/$formula/$version"
  found=0
  while IFS= read -r notice; do
    [[ -n "$notice" ]] || continue
    found=1
    install -m 644 "$notice" "$LICENSES/${formula}-${version}-$(basename "$notice")"
  done < <(find "$prefix" -maxdepth 1 -type f \( -iname 'license*' -o -iname 'copying*' -o -iname '*gpl*.txt' -o -iname '*apache*.txt' \) | sort)
  [[ "$found" == "1" ]] || {
    echo "license notice not found for bundled component: $formula $version" >&2
    exit 1
  }
done < "$components"

sign_artifact() {
  if [[ "$IDENTITY" == "-" ]]; then
    codesign --force --sign - "$1" >/dev/null
  else
    codesign --force --sign "$IDENTITY" --options runtime --timestamp "$1" >/dev/null
  fi
}
while IFS='|' read -r name source; do
  sign_artifact "$FRAMEWORKS/$name"
done < "$names"
sign_artifact "$HELPER"

{
  printf '%s\n' \
    'format=bridgevm-swtpm-runtime-v1' \
    "swtpm_version=$EXPECTED_SWTPM_VERSION" \
    "libtpms_version=$EXPECTED_LIBTPMS_VERSION" \
    'state_encryption=aes-256-cbc-etm/key-fd' \
    'upstream_swtpm=https://github.com/stefanberger/swtpm' \
    'upstream_libtpms=https://github.com/stefanberger/libtpms'
  while IFS='|' read -r formula version; do
    printf 'component|%s|%s\n' "$formula" "$version"
  done < "$components"
  printf 'artifact|Helpers/swtpm|%s\n' "$(shasum -a 256 "$HELPER" | awk '{ print $1 }')"
  while IFS='|' read -r name source; do
    printf 'artifact|Frameworks/%s|%s\n' "$name" "$(shasum -a 256 "$FRAMEWORKS/$name" | awk '{ print $1 }')"
  done < "$names"
} > "$MANIFEST"

verify_runtime
printf '%s\n' "$APP"
