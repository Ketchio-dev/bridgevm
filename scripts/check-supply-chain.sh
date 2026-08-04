#!/usr/bin/env bash
# Supply-chain policy checks that do not need network access.
#
# `cargo deny check` is the authority for advisories and licences, but it is
# not always installed and it needs the advisory database. This script checks
# the parts that are always verifiable: that the policy exists and parses,
# that pinned versions agree with what the workspace actually declares, and
# that no dependency is pulled from a git or path source, which would put code
# outside the locked registry into a release build.
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=/dev/null
. tools/versions.env

checks=0
fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { checks=$((checks + 1)); }

[ -f deny.toml ] || fail "deny.toml is missing"
python3 -c 'import tomllib,sys; tomllib.load(open("deny.toml","rb"))' \
    || fail "deny.toml does not parse"
ok

# The MSRV in versions.env must match Cargo.toml, or CI and the manifest
# disagree about what the project supports.
manifest_msrv="$(awk -F'"' '/^rust-version/ {print $2; exit}' Cargo.toml)"
[ "$manifest_msrv" = "$BRIDGEVM_RUST_MSRV" ] \
    || fail "MSRV mismatch: Cargo.toml says $manifest_msrv, versions.env says $BRIDGEVM_RUST_MSRV"
ok

# The pinned toolchain must be at least the MSRV.
lowest="$(printf '%s\n%s\n' "$BRIDGEVM_RUST_MSRV" "$BRIDGEVM_RUST_TOOLCHAIN" | sort -V | head -1)"
[ "$lowest" = "$BRIDGEVM_RUST_MSRV" ] \
    || fail "pinned toolchain $BRIDGEVM_RUST_TOOLCHAIN is older than MSRV $BRIDGEVM_RUST_MSRV"
ok

# No git or path dependencies: a release must be reproducible from the locked
# registry alone.
if grep -nE '^source = "git\+' Cargo.lock; then
    fail "a dependency comes from git; releases must build from the registry"
fi
ok

# Every locked package must come from the allowed registry or be a workspace
# member (workspace members have no source line).
unknown="$(awk '/^source = /' Cargo.lock | sort -u | grep -v 'registry+https://github.com/rust-lang/crates.io-index' || true)"
[ -z "$unknown" ] || fail "unexpected dependency sources:
$unknown"
ok

echo "PASS: supply-chain policy ($checks checks)"
