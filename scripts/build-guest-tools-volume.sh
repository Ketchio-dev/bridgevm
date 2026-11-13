#!/bin/bash
# Only OUT (an app-owned, absent path) is created; never opens a VM disk.
set -euo pipefail
assets="$1"; out="$2"
[[ ! -e "$out" ]]
work=$(mktemp -d "${TMPDIR:-/private/tmp}/bridgevm-guest-tools.XXXXXX")
device=""; pending=""
cleanup() {
  if [[ -n "$device" ]]; then hdiutil detach "$device" -quiet || true; fi
  if [[ -n "$pending" ]]; then rm -f "$pending"; fi
  rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/files" "$work/mount"
for name in bvagent.ps1 bvagent-input.ps1 bvagent-unicode-input.cs bvagent-key-input.cs bvagent-pointer-input.cs bvagent-window-inventory.ps1 bv-window-inventory.cs bvagent-task.ps1 bvagent-update.ps1 install.cmd bvagent-install.ps1; do
  cp -X "$assets/$name" "$work/files/$name"
done
# Content-sized FAT media, rounded up for partition/filesystem overhead.
kb=$(du -sk "$work/files" | awk '{print $1}')
mb=$(( (kb + 1023) / 1024 + 1 ))
(( mb >= 8 )) || mb=8
hdiutil create -quiet -size "${mb}m" -fs 'MS-DOS FAT16' -layout MBRSPUD -volname BVTOOLS "$work/tools.dmg"
hdiutil attach -nobrowse -mountpoint "$work/mount" "$work/tools.dmg" > "$work/attach"
device=$(awk 'NR==1{print $1}' "$work/attach")
cp -X "$work/files/"* "$work/mount/"
/usr/sbin/dot_clean -m "$work/mount"
hdiutil detach "$device" -quiet
device=""
hdiutil convert -quiet -format UDTO "$work/tools.dmg" -o "$work/raw"
# Publish only a complete image even when output is on a different volume.
pending=$(mktemp "${out}.XXXXXX")
cp -X "$work/raw.cdr" "$pending"
chmod 400 "$pending"
mv "$pending" "$out"
