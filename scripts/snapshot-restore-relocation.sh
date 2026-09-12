#!/usr/bin/env bash
# Source only. Previous gate phases have confirmed the guest is powered off.
snapshot_relocate_live() {
  local destination="$OUT/relocated-live"
  [[ -d "$WORK" && ! -L "$WORK" && -d "$OUT" ]] || return 1
  [[ ! -e "$destination" && ! -L "$destination" ]] || return 1
  mv -- "$WORK" "$destination" || return 1
  WORK=$destination
  printf 'relocated live pair: %s\n' "$WORK"
}
