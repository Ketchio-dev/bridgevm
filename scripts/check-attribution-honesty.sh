#!/usr/bin/env bash
# Keep the repository's copyright position true and self-consistent.
#
# Reject unfounded derivation claims and unrecorded licence obligations.
#
# This checks what can be verified from the tree itself. Bundle-level proof
# (dynamic linkage of LGPL dylibs, the Rust licence inventory) is the job of
# scripts/verify-app-third-party-notices.sh, which runs against a built app.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

errors=0

fail() {
  echo "$1" >&2
  errors=$((errors + 1))
}

[[ -f LICENSE ]] || fail "LICENSE is missing"
[[ -f THIRD-PARTY-NOTICES.md ]] || fail "THIRD-PARTY-NOTICES.md is missing"

# The notices file must keep answering the questions a licence review asks:
# what BridgeVM's own code is, and what it does about guest operating systems.
for required in \
  "## Provenance of BridgeVM's own code" \
  "## Guest operating systems"; do
  grep -qF "$required" THIRD-PARTY-NOTICES.md ||
    fail "THIRD-PARTY-NOTICES.md no longer contains: $required"
done

# No tracked file may be an operating-system image. BridgeVM redistributes no
# guest OS, and a committed image would silently contradict that.
if git ls-files | grep -Ei '\.(iso|vhdx?|wim|esd)$' | grep -q .; then
  git ls-files | grep -Ei '\.(iso|vhdx?|wim|esd)$' >&2
  fail "an operating-system image is tracked in git"
fi

# Claiming derivation from a specific other VMM or graphics stack is exactly
# the unfounded admission this check exists to prevent. Interface names are
# fine; it is the derivation verb next to a product name that is not.
derivation_hits=$(
  grep -rniE \
    '(derived from|based on|ported from|adapted from|copied from|borrowed from|taken from|fork of)[^.]{0,40}\b(qemu|crosvm|u[t]m|parallels|vmware|virtualbox|bochs|xen)\b' \
    --include='*.rs' --include='*.swift' --include='*.md' \
    --exclude-dir=.git --exclude-dir=target --exclude-dir=build \
    --exclude-dir=archive \
    --exclude=THIRD-PARTY-NOTICES.md \
    . 2>/dev/null || true
  grep -rniE \
    '(qemu.{0,80}(oracle|parity|authoritative|answer[ -]?key|reference implementation|source of truth|match|mirror|copy|transcrib|borrow)|(oracle|parity|authoritative|answer[ -]?key|reference implementation|source of truth|match|mirror|copy|transcrib|borrow).{0,80}qemu)' \
    --include='*.rs' crates/bridgevm-hvf 2>/dev/null || true
)
if [[ -n "$derivation_hits" ]]; then
  echo "$derivation_hits" >&2
  fail "a file claims BridgeVM code derives from another product"
fi

# virglrenderer is the one third-party component BridgeVM modifies. The patch
# must stay in the tree so the modification is visible rather than folded into
# a shipped binary, and the notices must keep saying so.
patch="scripts/patches/virglrenderer-macos-venus.patch"
[[ -s "$patch" ]] || fail "the virglrenderer patch is missing: $patch"
grep -qF "$patch" THIRD-PARTY-NOTICES.md ||
  fail "THIRD-PARTY-NOTICES.md no longer points at the virglrenderer patch"

# Every licence text the notices promise to ship must exist in the tree.
for license_text in docs/licenses/virglrenderer-MIT.txt docs/licenses/libepoxy-MIT.txt; do
  [[ -s "$license_text" ]] || fail "a promised licence text is missing: $license_text"
done

# The permissive-only allowlist is what keeps a copyleft dependency from
# entering a statically linked distribution unnoticed.
for copyleft in GPL-2.0 GPL-3.0 LGPL-2.1 LGPL-3.0 AGPL-3.0 MPL-2.0; do
  if grep -qE "^[[:space:]]*\"$copyleft" deny.toml; then
    fail "deny.toml allows a copyleft licence for statically linked crates: $copyleft"
  fi
done

# A known defect that is still unfixed has to stay disclosed where a reader
# meets the product, not only in a dated evidence file they would have to go
# looking for. Deleting the README paragraph is otherwise invisible to every
# other gate, which makes the product look better than it is.
# The evidence file's own Status line is the source of truth: while it still
# says the title/tab/menu glyphs do not appear, the README must link it. When
# that defect is actually fixed, that line changes and this check retires with
# it.
glyph_evidence="docs/windows-arm/evidence/windows-glyph-text-integer-attributes-20260814.md"
if [[ -f "$glyph_evidence" ]] && grep -qF 'glyphs still do not' "$glyph_evidence"; then
  grep -qF "$glyph_evidence" README.md ||
    fail "README.md no longer discloses the open glyph rendering defect: $glyph_evidence"
fi

if [[ $errors -ne 0 ]]; then
  echo "attribution honesty: FAIL ($errors error(s))" >&2
  exit 1
fi

echo "attribution honesty: PASS"
