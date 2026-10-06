#!/usr/bin/env bash
# D10 archives use a clean cache-free reader matching their sealed job source.
set -euo pipefail
HERE="$(cd "${BASH_SOURCE[0]%/*}" && pwd -P)"
ROOT="$(cd "$HERE/../.." && pwd -P)"
DIR="${1:?job directory}"; JOB_ID="${2:?job id}"; COMMIT="${3:?sealed source}"
/bin/bash --noprofile --norc -p "$HERE/t22-pair-source-admission.sh" "$ROOT" "$COMMIT" || exit 1
exec /usr/bin/env -i HOME="$HOME" PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin \
    python3 -B -s -E "$HERE/t22_pair_archive_entry.py" "$DIR" "$JOB_ID" "$COMMIT"
