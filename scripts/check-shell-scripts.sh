#!/usr/bin/env bash
# Static analysis for the shell scripts that implement this project's gates.
#
# The gates themselves are shell, so a defect here silently weakens every claim
# they make. This guard exists because of one such defect: check-project.sh and
# four verification scripts ran `cd "$ROOT"` without checking it, and none of
# them set -e. A failed cd therefore left the script running in whatever
# directory the caller happened to be in, and it still exited 0 -- a gate that
# reports PASS against the wrong tree.
#
# Only genuine-defect classes are enforced -- unchecked cd, "rm -rf ${x}/" with
# an empty variable, unquoted expansions, indirect $?, unassigned variables --
# because a gate that fires on style noise gets disabled rather than fixed.
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

printf 'shell scripts: PASS (%d scripts, %s)\n' "${#scripts[@]}" "$CHECKS"
