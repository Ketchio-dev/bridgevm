#!/usr/bin/env bash
# Static analysis for the shell scripts that implement this project's gates.
#
# The gates are shell, so a defect here weakens every claim they make. Five
# scripts ran `cd "$ROOT"` unchecked with no set -e, so a failed cd left them
# checking the wrong tree and still exiting 0. Only genuine-defect classes are
# enforced -- a gate that fires on style noise gets disabled rather than fixed.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

if ! command -v shellcheck >/dev/null 2>&1; then
  # A gate that skips silently is not a gate. Locally it is reasonable not to
  # require the tool, but CI must never report PASS without having run it.
  if [[ -n "${CI:-}" || -n "${BRIDGEVM_REQUIRE_SHELLCHECK:-}" ]]; then
    printf 'shell scripts: FAIL (shellcheck is required here and is not installed)\n' >&2
    exit 1
  fi
  printf 'shell scripts: SKIP (shellcheck not installed)\n'
  exit 0
fi

# Enforced defect classes. Adding one here is a ratchet: it may only be added
# once the tree is already clean of it.
CHECKS='SC2164,SC2115,SC2068,SC2086,SC2181,SC2154'

# bash 3.2 ships with macOS and has no mapfile.
scripts=()
while IFS= read -r script; do
  scripts+=("$script")
done < <(git ls-files '*.sh')
if [[ ${#scripts[@]} -eq 0 ]]; then
  printf 'shell scripts: FAIL (no scripts found)\n' >&2
  exit 1
fi

# shellcheck costs ~61 ms per file across 291 files, so one invocation took
# 17.7 s -- over half of `check-project.sh --fast`. Splitting across cores gives
# 4.6 s; measured -P 4/8/16 at 6.0/4.6/4.5 s, so eight saturates it. Sourced
# libraries have no shebang by design, hence --shell=bash. xargs returns 123
# when a child does, which here means findings, so the findings decide.
output=$(printf '%s\n' "${scripts[@]}" | xargs -P 8 -n 30 \
  shellcheck --shell=bash --external-sources \
  --severity=style --include="$CHECKS" --format=gcc 2>/dev/null)

if [[ -n "$output" ]]; then
  printf '%s\n' "$output" >&2
  printf 'shell scripts: FAIL (%d finding(s) in %d file(s))\n' \
    "$(printf '%s\n' "$output" | wc -l | tr -d ' ')" \
    "$(printf '%s\n' "$output" | cut -d: -f1 | sort -u | wc -l | tr -d ' ')" >&2
  exit 1
fi

# A Cellar path pins a formula version: one named qemu 11.0.1 on a machine
# already running 11.0.2.
if git grep -nE '"[^"]*/Cellar/[^"]+/[0-9]+\.[0-9]+' -- '*.rs' '*.swift'; then
  printf 'shell scripts: FAIL (version-pinned Cellar path)\n' >&2; exit 1
fi

printf 'shell scripts: PASS (%d scripts, %s)\n' "${#scripts[@]}" "$CHECKS"
