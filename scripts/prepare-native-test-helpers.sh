#!/usr/bin/env bash
# Sourced by both native test entry points; ROOT is the repository directory.
BRIDGEVM_TEST_SNAPSHOT_HELPER="$(bash "$ROOT/scripts/prepare-hvf-import-test-helper.sh")"
BRIDGEVM_TEST_OWNED_RUNNER="$(bash "$ROOT/scripts/prepare-owned-runner-test-helper.sh")"
BRIDGEVM_TEST_RUST_CLI="$(dirname "$BRIDGEVM_TEST_OWNED_RUNNER")/bridgevm"
swift build --package-path "$ROOT/apps/macos" --product BridgeVMControl >&2
BRIDGEVM_TEST_NATIVE_CLI="$(swift build --package-path "$ROOT/apps/macos" --show-bin-path)/BridgeVMControl"
export BRIDGEVM_TEST_SNAPSHOT_HELPER BRIDGEVM_TEST_OWNED_RUNNER BRIDGEVM_TEST_NATIVE_CLI BRIDGEVM_TEST_RUST_CLI
