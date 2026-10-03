#!/usr/bin/env bash

absolute_media_path() {
  local path="$1"
  local dir
  local base
  case "$path" in
    /*) ;;
    *) path="$PWD/$path" ;;
  esac
  dir="$(dirname "$path")"
  base="$(basename "$path")"
  if [[ -d "$dir" ]]; then
    (cd "$dir" && printf '%s/%s\n' "$(pwd -P)" "$base")
  else
    printf '%s\n' "$path"
  fi
}

path_has_parent_component() {
  case "$1" in
    ..|../*|*/..|*/../*) return 0 ;;
    *) return 1 ;;
  esac
}

require_destructive_media_path() {
  local label="$1"
  local path
  if path_has_parent_component "$2"; then
    echo "FAIL: destructive $label path must not contain '..' components: $2" >&2
    exit 2
  fi
  path="$(absolute_media_path "$2")"
  case "$path" in
    /tmp/bridgevm-*|/private/tmp/bridgevm-*) ;;
    *)
      echo "FAIL: destructive $label path must be under /tmp/bridgevm-*: $2" >&2
      exit 2
      ;;
  esac
  case "$path" in
    /tmp/bridgevm-c3-unattend-target.raw|/private/tmp/bridgevm-c3-unattend-target.raw|\
    /tmp/bridgevm-c3-unattend-vars.fd|/private/tmp/bridgevm-c3-unattend-vars.fd|\
    /tmp/bridgevm-c3-placeholder-nsid1.raw|/private/tmp/bridgevm-c3-placeholder-nsid1.raw)
      echo "FAIL: destructive $label path matches preserved source media: $2" >&2
      exit 2
      ;;
  esac
}

select_scripted_install_firmware() {
  local runtime_root="$1"
  local expected_sha="b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
  local size hash_output
  case "$runtime_root" in
    */Contents/Resources) FIRMWARE_CODE="$runtime_root/firmware/edk2-aarch64-secure-code.fd" ;;
    *) FIRMWARE_CODE="$runtime_root/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd" ;;
  esac
  [[ -f "$FIRMWARE_CODE" && ! -L "$FIRMWARE_CODE" && -r "$FIRMWARE_CODE" ]] || {
    echo "FAIL: pinned firmware must be a readable regular file: $FIRMWARE_CODE" >&2
    return 1
  }
  size="$(/usr/bin/wc -c < "$FIRMWARE_CODE")" || return 1
  [[ "${size//[[:space:]]/}" == "3145728" ]] || {
    echo "FAIL: pinned firmware must be exactly 3 MiB: $FIRMWARE_CODE" >&2
    return 1
  }
  hash_output="$(/usr/bin/shasum -a 256 -- "$FIRMWARE_CODE")" || return 1
  FIRMWARE_SHA256="${hash_output%% *}"
  [[ "$FIRMWARE_SHA256" == "$expected_sha" ]] || {
    echo "FAIL: firmware digest does not match the pinned Secure Boot + TPM2 build" >&2
    return 1
  }
}
