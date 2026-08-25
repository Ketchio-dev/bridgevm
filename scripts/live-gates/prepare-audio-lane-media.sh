#!/usr/bin/env bash
# Clone immutable canonical audio media into one private writable lane.
set -euo pipefail
prepare() {
  local target=$1 vars=$2 work=$3
  rm -rf "$work"; mkdir -p "$work"
  cp -c "$target" "$work/disk.raw"; cp "$vars" "$work/vars.fd"
  chflags nouchg,noschg "$work/disk.raw" "$work/vars.fd"
  chmod 600 "$work/disk.raw" "$work/vars.fd"
}
if [[ ${1:-} == --self-test ]]; then
  root=$(mktemp -d); trap 'chflags -R nouchg,noschg "$root" 2>/dev/null || true; rm -rf "$root"' EXIT
  printf disk >"$root/source.raw"; printf vars >"$root/source.fd"
  chmod 400 "$root/source.raw" "$root/source.fd"; chflags uchg "$root/source.raw" "$root/source.fd"
  before=$(shasum -a 256 "$root/source.raw" "$root/source.fd")
  prepare "$root/source.raw" "$root/source.fd" "$root/lane"
  printf x >>"$root/lane/disk.raw"; printf y >>"$root/lane/vars.fd"
  [[ $(stat -f '%Sp' "$root/lane/disk.raw") == -rw------- ]]
  [[ $(stat -f '%Sf' "$root/source.raw") == *uchg* && $(shasum -a 256 "$root/source.raw" "$root/source.fd") == "$before" ]]
  rm -rf "$root/lane"; echo 'PASS: immutable audio sources make writable removable lane media'; exit 0
fi
[[ $# == 3 ]] || { echo 'usage: prepare-audio-lane-media.sh TARGET VARS WORK' >&2; exit 2; }
prepare "$1" "$2" "$3"
