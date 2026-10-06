#!/bin/bash
set -euo pipefail
HERE="$(cd "${BASH_SOURCE[0]%/*}" && pwd -P)"
ROOT="$(cd "$HERE/../.." && pwd -P)"
exec /bin/bash --noprofile --norc -p "$HERE/d11-fixture-queue-dispatch.sh" \
    archive "${1:?directory}" "$ROOT" "${3:?commit}" "${2:?job}"
