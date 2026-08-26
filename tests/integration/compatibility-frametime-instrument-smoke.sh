#!/usr/bin/env bash
# The 20-workload matrix needs one generic frame-time instrument whose raw
# output the host validator accepts. The previous script hardcoded PPSSPP and
# emitted only summary scalars, so no row could carry retained raw evidence.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"; cd "$ROOT"
FT=scripts/win-assets/bvgpu-frametime-series.ps1
fail() { echo "FAIL: $*" >&2; exit 1; }
[[ -f "$FT" ]] || fail 'frame-time instrument is absent'
python3 -c "
import sys
d=open('$FT','rb').read()
assert d.count(b'\r\n') and d.count(b'\n')==d.count(b'\r\n'), 'win-assets must be pure CRLF'
t=d.decode()
for name in ('\$Id','\$ProcessName','\$OutDir','\$WarmupSeconds','\$MeasureSeconds'):
    assert name in t, name
assert 'PPSSPP' not in t, 'the instrument must not hardcode one title'
assert 'frametimes-ms' in t and 'series_sha256' in t
" || fail 'instrument is not generic or emits no raw series'
errs=$(pwsh -NoProfile -Command '
$e=$null;$k=$null
$null=[System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path '"'$FT'"').Path,[ref]$k,[ref]$e)
if ($e.Count) { $e | ForEach-Object { $_.Message } }') || true
[[ -z "$errs" ]] || fail "instrument does not parse: $errs"
# The guest's tick->ms conversion, nearest-rank quantiles and SHA-256 must agree
# with scripts/validate-windows-compatibility-matrix.py exactly, or a row's
# reported scalars can never match its retained series.
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
guest=$(pwsh -NoProfile -Command "
\$ticks=@(0L,166667L,333334L,500001L,5000000L)
\$lines=New-Object System.Collections.Generic.List[string]
for (\$i=1; \$i -lt \$ticks.Count; \$i++) { \$ms=(\$ticks[\$i]-\$ticks[\$i-1])/10000.0
  if (\$ms -gt 0 -and \$ms -le 60000) { \$lines.Add(\$ms.ToString('0.###',[System.Globalization.CultureInfo]::InvariantCulture)) } }
[System.IO.File]::WriteAllText('$work/probe.frametimes-ms', (\$lines -join \"\`n\") + \"\`n\")
\$s=\$lines | ForEach-Object { [double]\$_ } | Sort-Object
\$q={param(\$p) \$s[[int][Math]::Floor((\$s.Count-1)*\$p)]}
\"\$(\$lines.Count) \$(& \$q 0.50) \$(& \$q 0.95) \$(& \$q 0.99) \$((Get-FileHash -LiteralPath '$work/probe.frametimes-ms' -Algorithm SHA256).Hash.ToLower())\"")
python3 - "$work/probe.frametimes-ms" "$guest" <<'PY' || fail 'guest series and host validator disagree'
import hashlib,math,sys
raw=open(sys.argv[1],'rb').read(); v=sorted(float(x) for x in raw.splitlines())
assert v and all(math.isfinite(x) and 0<x<=60000 for x in v)
host=[str(len(v))]+[repr(v[int((len(v)-1)*q)]) for q in (.5,.95,.99)]+[hashlib.sha256(raw).hexdigest()]
g=sys.argv[2].split()
assert g[0]==host[0] and [float(x) for x in g[1:4]]==[float(x) for x in host[1:4]] and g[4]==host[4], (g,host)
PY
echo 'PASS: generic frame-time instrument agrees with the compatibility validator'
