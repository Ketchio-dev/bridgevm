#!/usr/bin/env bash
# A child process avoids exported shell-function compiler adapters.
set -euo pipefail
case "${BV_XCTEST_SANITIZER:-}" in
    "") exec swiftc "$@" ;;
    thread) exec swiftc -sanitize=thread "$@" ;;
    *) printf 'Unsupported XCTest sanitizer: %s\n' "$BV_XCTEST_SANITIZER" >&2; exit 2 ;;
esac
