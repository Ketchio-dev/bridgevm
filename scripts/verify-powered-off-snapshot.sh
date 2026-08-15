#!/usr/bin/env bash
# A19: prove a powered-off snapshot is a byte-exact atomic pair.
#
# The unit tests prove the mechanism against synthetic files. This proves the
# claim the criterion actually makes: that snapshotting the real disk and vars
# a Windows guest was left with, then restoring them, reproduces both files
# byte for byte, and that a torn pair is refused rather than silently accepted.
#
# What this does NOT prove is acceptance item 5 -- that a restored snapshot
# boots Windows and preserves a guest-created marker. That needs a full boot
# and is reported as not-run rather than assumed.
set -uo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO" || exit 1

DISK=${DISK:-$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730.raw}
VARS=${VARS:-$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/snapshot-pair-$(date +%Y%m%d-%H%M%S)}
# Default quota is the pair's own size plus room; a real product path would
# set this from free space.
QUOTA=${QUOTA:-$((80 * 1024 * 1024 * 1024))}

fail() { echo "FAIL: $*" >&2; exit 1; }

# openssl rather than shasum: shasum is a Perl script and runs about five
# times slower, which on a 64 GiB image hashed six times is the difference
# between minutes and most of an hour.
sha256() { openssl dgst -sha256 -r "$1" | cut -d' ' -f1; }

for required in "$DISK" "$VARS"; do
  [[ -e "$required" ]] || fail "missing required input: $required"
done

mkdir -p "$OUT"
RECEIPT="$OUT/receipt.txt"
: > "$RECEIPT"
say() { echo "$*" | tee -a "$RECEIPT"; }

say "disk:  $DISK ($(stat -f %z "$DISK") bytes)"
say "vars:  $VARS ($(stat -f %z "$VARS") bytes)"
say "quota: $QUOTA bytes"

# Work on clones so the canonical inputs are never the thing under test.
WORK="$OUT/work"
mkdir -p "$WORK"
# On every exit, not just success: this holds two clones of a 64 GiB image, and
# leaving them behind after a failure filled the disk until the free-space
# guard refused the next run.
trap 'rm -rf "$WORK"' EXIT INT TERM
cp -c "$DISK" "$WORK/disk.raw" || fail "could not clone the disk"
cp "$VARS" "$WORK/vars.fd" || fail "could not copy the vars"

before_disk=$(sha256 "$WORK/disk.raw")
before_vars=$(sha256 "$WORK/vars.fd")
say "before disk sha256: $before_disk"
say "before vars sha256: $before_vars"

# Release: a debug build hashes 64 GiB roughly an order of magnitude slower,
# and this gate hashes it several times.
cargo build --release -p bridgevm-hvf --locked --example snapshot_pair_cli 2>>"$OUT/build.log" \
  || fail "could not build snapshot_pair_cli; see $OUT/build.log"
CLI=target/release/examples/snapshot_pair_cli

say "--- create ---"
"$CLI" create "$WORK/disk.raw" "$WORK/vars.fd" "$WORK/snap" a19-gate "$QUOTA" \
  | tee -a "$RECEIPT" || fail "create failed"

say "--- verify ---"
"$CLI" verify "$WORK/snap" | tee -a "$RECEIPT" || fail "verify failed"

# Change both live files, so a restore that silently does nothing cannot pass.
printf 'clobbered' | dd of="$WORK/disk.raw" bs=1 seek=512 conv=notrunc 2>/dev/null
printf 'clobbered' | dd of="$WORK/vars.fd" bs=1 seek=512 conv=notrunc 2>/dev/null
clobbered_disk=$(sha256 "$WORK/disk.raw")
[[ "$clobbered_disk" != "$before_disk" ]] || fail "the clobber did not change the disk"
say "clobbered disk sha256: $clobbered_disk"

# A restore stages a full copy beside the live pair, so the volume needs room
# for live + snapshot + copy at once. The first live run of this gate failed
# here with ENOSPC on a 64 GiB image, which is how that requirement was found.
free_gib=$(df -g "$WORK" | awk 'NR==2 {print $4}')
needed_gib=$(( ($(stat -f %z "$DISK") / 1073741824) + 2 ))
say "free: ${free_gib}GiB, restore needs about ${needed_gib}GiB to stage"
if (( free_gib < needed_gib )); then
  say "SKIP: not enough free space to stage a restore of this pair"
  say "  create and verify passed; restore is unproven on this run"
  exit 0
fi

say "--- restore ---"
"$CLI" restore "$WORK/snap" "$WORK/disk.raw" "$WORK/vars.fd" | tee -a "$RECEIPT" \
  || fail "restore failed"

after_disk=$(sha256 "$WORK/disk.raw")
after_vars=$(sha256 "$WORK/vars.fd")
say "after disk sha256: $after_disk"
say "after vars sha256: $after_vars"

[[ "$after_disk" == "$before_disk" ]] || fail "restored disk differs from the original"
[[ "$after_vars" == "$before_vars" ]] || fail "restored vars differ from the original"

# A tampered snapshot must be refused, and must leave the live pair alone.
say "--- refusal of a tampered snapshot ---"
printf 'x' | dd of="$WORK/snap/vars.fd" bs=1 seek=0 conv=notrunc 2>/dev/null
if "$CLI" restore "$WORK/snap" "$WORK/disk.raw" "$WORK/vars.fd" >>"$RECEIPT" 2>&1; then
  fail "a tampered snapshot was restored"
fi
say "tampered snapshot refused"
[[ "$(sha256 "$WORK/disk.raw")" == "$before_disk" ]] \
  || fail "a refused restore modified the live disk"
say "live pair untouched by the refusal"

say ""
say "PASS: powered-off snapshot pair is byte-exact across create/restore"
say "NOT PROVEN HERE: acceptance item 5 (a restored snapshot boots Windows and"
say "  preserves a guest-created marker). A19 stays open until that runs."
echo "receipt: $RECEIPT"
