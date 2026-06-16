#!/usr/bin/env bash

bridgevm_validate_macos_app_name() {
  local name="$1"
  local env_name="${2:-BRIDGEVM_MACOS_APP_NAME}"

  if [[ -z "${name//[[:space:]]/}" ]]; then
    echo "$env_name must be a non-empty .app bundle basename." >&2
    return 2
  fi
  case "$name" in
    */*)
      echo "$env_name must be a basename, not a path: $name" >&2
      return 2
      ;;
    .|..)
      echo "$env_name must not be '.' or '..'." >&2
      return 2
      ;;
    *.app)
      echo "$env_name must not include the .app suffix: $name" >&2
      return 2
      ;;
  esac
}
