#!/usr/bin/env bash
# Run under privileged Bash; no repository Python is imported by admission.
set -euo pipefail
source "${BASH_SOURCE[0]%/*}/t22-pair-source-environment.sh"
ROOT="${1:?source root}"; COMMIT="${2:?source commit}"
[[ "$COMMIT" =~ ^[0-9a-f]{40}$ && -d "$ROOT" && ! -L "$ROOT" ]] || exit 1
OBSERVED="$(checked_git rev-parse HEAD)" || exit 1
[[ "$OBSERVED" == "$COMMIT" ]] || exit 1
CHANGED="$(checked_git status --porcelain --untracked-files=all)" || exit 1
[[ -z "$CHANGED" ]] && /bin/bash --noprofile --norc -p "${BASH_SOURCE[0]%/*}/t22-pair-cache-admission.sh" "$ROOT"
