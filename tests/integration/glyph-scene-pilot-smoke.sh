#!/usr/bin/env bash
set -euo pipefail; ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="$ROOT/scripts/live-gates/run-glyph-scene-pilot-tier.sh"; CASE="$ROOT/scripts/run-glyph-scene-pilot-case.sh"
[[ -x "$TIER" && -x "$CASE" && -x "$ROOT/scripts/glyph_region_analysis.py" ]]
grep -Fq 'criterion":"glyph-diagnostic-only"' "$TIER"
grep -Fq 'sample_count":1' "$TIER"
grep -Fq 'glyph correctness remains unmeasured' "$TIER"
grep -Fq 'glyph_correctness=unmeasured' "$CASE"
grep -Fq 'active-scanout.fb.iosurface' "$CASE"
grep -Fq 'rect=50,60,700,500' "$CASE"
grep -Fq -- '--ready "$CASE/glyph-capture.ready"' "$CASE"
! grep -Eq 'glyph_correctness=(pass|true)|ocr.*pass|components.*(pass|threshold)|nc +-l|socat|TcpListener|sudo|actions-runner' "$TIER" "$CASE"
"$ROOT/tests/integration/glyph-region-observation-smoke.py" | grep -q PASS
echo 'PASS: glyph scene pilot stays diagnostic-only and active-CGL'
