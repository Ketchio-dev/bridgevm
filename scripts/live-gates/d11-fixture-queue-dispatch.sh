#!/bin/bash
# Admit clean exact source before importing any D11 repository Python.
set -euo pipefail
MODE="${1:?mode}"; shift
HERE="$(cd "${BASH_SOURCE[0]%/*}" && pwd -P)"
EXPECTED="$(cd "$HERE/../.." && pwd -P)"
case "$MODE" in
    validate|seal) ROOT="$EXPECTED"; COMMIT="${2:?commit}" ;;
    run|finalize|publish|guard|archive) ROOT="${2:?root}"; COMMIT="${3:?commit}" ;;
    *) exit 2 ;;
esac
[[ "$ROOT" == "$EXPECTED" ]] || exit 1
/bin/bash --noprofile --norc -p "$HERE/t22-pair-source-admission.sh" "$ROOT" "$COMMIT"
exec /usr/bin/env -i HOME="$HOME" PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin \
    PYTHONNOUSERSITE=1 python3 -B -s -E "$HERE/d11_fixture_queue.py" "$MODE" "$@"
