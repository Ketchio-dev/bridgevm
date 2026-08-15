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
# Only genuine-defect classes are enforced. Style noise is not, because a gate
# that fires on harmless findings gets disabled rather than fixed:
#
#   SC2164  unchecked cd (the defect above)
#   SC2115  "rm -rf ${x}/" with a possibly-empty variable
#   SC2068  unquoted $@ (word splitting on paths with spaces)
#   SC2086  unquoted expansion in a test or command word
#   SC2181  checking $? indirectly instead of the command
#   SC2154  referencing a variable that is never assigned
#
# Everything else shellcheck reports is advisory and deliberately not gated.
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

# Sourced libraries have no shebang by design, so tell shellcheck the dialect
# rather than requiring a shebang they must not have.
output=$(shellcheck --shell=bash --external-sources \
  --severity=style --include="$CHECKS" --format=gcc "${scripts[@]}" 2>/dev/null)

if [[ -n "$output" ]]; then
  printf '%s\n' "$output" >&2
  printf 'shell scripts: FAIL (%d finding(s) in %d file(s))\n' \
    "$(printf '%s\n' "$output" | wc -l | tr -d ' ')" \
    "$(printf '%s\n' "$output" | cut -d: -f1 | sort -u | wc -l | tr -d ' ')" >&2
  exit 1
fi

printf 'shell scripts: PASS (%d scripts, %s)\n' "${#scripts[@]}" "$CHECKS"
