#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$ROOT/scripts/snapshot-restore-relocation.sh"
fixture=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-relocation-contract.XXXXXX")
trap 'rm -r "$fixture"' EXIT
new_case() {
  OUT="$fixture/$1"
  WORK="$OUT/live"
  mkdir -p "$WORK"
  printf logical-disk > "$WORK/disk.raw"
  printf logical-vars > "$WORK/vars.fd"
}
expect_refused() {
  local before=$WORK
  if snapshot_relocate_live; then printf 'FAIL: relocation should refuse\n' >&2; exit 1; fi
  [[ "$WORK" == "$before" && -f "$WORK/disk.raw" && -f "$WORK/vars.fd" ]]
}

new_case complete
mkdir -p "$WORK/.bridgevm-pair-v2-fixture/current"
printf restored-disk > "$WORK/.bridgevm-pair-v2-fixture/current/disk.raw"
printf restored-vars > "$WORK/.bridgevm-pair-v2-fixture/current/vars.fd"
snapshot_relocate_live
[[ "$WORK" == "$OUT/relocated-live" && ! -e "$OUT/live" ]]
[[ $(cat "$WORK/.bridgevm-pair-v2-fixture/current/disk.raw") == restored-disk ]]
[[ $(cat "$WORK/.bridgevm-pair-v2-fixture/current/vars.fd") == restored-vars ]]
[[ $(cat "$WORK/disk.raw") == logical-disk && $(cat "$WORK/vars.fd") == logical-vars ]]
expect_refused

new_case existing
mkdir "$OUT/relocated-live"
printf preserve > "$OUT/relocated-live/sentinel"
expect_refused
[[ $(cat "$OUT/relocated-live/sentinel") == preserve ]]

new_case dangling
ln -s "$OUT/missing-target" "$OUT/relocated-live"
expect_refused
[[ -L "$OUT/relocated-live" ]]

new_case source_link
mv "$WORK" "$OUT/real-source"
ln -s "$OUT/real-source" "$WORK"
expect_refused
[[ -L "$WORK" && ! -e "$OUT/relocated-live" ]]
printf 'PASS: relocation contract (complete hidden pair, repeat, existing target, dangling target, source symlink)\n'
