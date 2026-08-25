#!/usr/bin/env bash
set -euo pipefail; ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TIER="$ROOT/scripts/live-gates/run-glyph-scene-pilot-tier.sh"; CASE="$ROOT/scripts/run-glyph-scene-pilot-case.sh"
[[ -x "$TIER" && -x "$CASE" && -x "$ROOT/scripts/glyph_region_analysis.py" ]]
grep -Fq 'criterion":"glyph-diagnostic-only"' "$TIER"
grep -Fq 'sample_count":1' "$TIER"
grep -Fq 'glyph correctness remains unmeasured' "$TIER"
grep -Fq 'glyph_correctness=unmeasured' "$CASE"
grep -Fq 'active-scanout.fb.iosurface' "$CASE"
grep -Fq 'rect=50,60,700,500' "$CASE"; python3 -c 'import re,sys;t=open(sys.argv[1],encoding="utf-8").read();m=re.search(r"for chunk in ([0-9a-f ]+); do",t);assert m;c=[bytes.fromhex(x) for x in m.group(1).split()];assert [len(x) for x in c]==[32,32,32];assert b"".join(c)==b"BridgeVM Glyph Probe ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*()"' "$CASE"
grep -Fq -- '--ready "$CASE/glyph-capture.ready"' "$CASE"; grep -Fq 'timeout="$3" deadline n log; deadline=$((SECONDS+timeout))' "$CASE"; ! grep -Fq 'timeout="$3" deadline=$((' "$CASE"; fn=$(grep '^wait_for(){' "$CASE"); d=$(mktemp -d); printf 'MATCH\n' >"$d/run.log"; RUN=$d; pid=$$; eval "$fn"; wait_for '^MATCH$' 1 1; rm -rf "$d"
! grep -Eq 'glyph_correctness=(pass|true)|ocr.*pass|components.*(pass|threshold)|nc +-l|socat|TcpListener|sudo|actions-runner' "$TIER" "$CASE"
python3 -c 'import re,sys;p=re.search(r"for chunk in ([0-9a-f ]+); do",open(sys.argv[1]).read()).group(1).split();assert "".join(p)=="427269646765564d20476c7970682050726f6265204142434445464748494a4b4c4d4e4f505152535455565758595a206162636465666768696a6b6c6d6e6f707172737475767778797a20303132333435363738392021402324255e262a2829";assert max(len("text-hex:"+x) for x in p)<=128;assert "SETUP_INPUT_ENV_MAX_BYTES: usize = 128" in open(sys.argv[2]).read()' "$CASE" "$ROOT/crates/bridgevm-hvf/examples/hvf_gic_boot_probe/xhci_hid_input/setup_input.rs"; "$ROOT/tests/integration/glyph-region-observation-smoke.py" | grep -q PASS
echo 'PASS: glyph scene pilot stays diagnostic-only and active-CGL'
