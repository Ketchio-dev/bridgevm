"""Check source wiring only; native comparison and live WinPE have separate proofs."""
from pathlib import Path
root = Path(__file__).resolve().parents[2]
guest = (root / "scripts/win-assets/bvinject.cmd").read_text()
builder = (root / "scripts/build-hvf-windows-driver-injector.sh").read_text()
helper = (root / "scripts/injector-file-compare-build.sh").read_text()
assert 'source "$(dirname "${BASH_SOURCE[0]}")/injector-file-compare-build.sh"' in builder
assert '"$ASSETS/bv-file-compare.c" -o "$DST_VOL/bv-file-compare.exe"' in helper
call = '"%DRV%\\..\\bv-file-compare.exe" "%DRV%\\..\\bvgpu-clean-driver-state.ps1" "%WIN%\\BridgeVM\\viogpu3d\\bvgpu-clean-driver-state.ps1" >nul'
assert guest.count(call) == 1
index = guest.index(call)
guard = '\n  if errorlevel 1 (\n    echo BVINJECT ERROR: package-local cleanup byte comparison failed\n    goto :end\n  )'
assert guest[index + len(call):].startswith(guard)
assert index < guest.index('echo pending > %WIN%\\BridgeVM\\viogpu3d-firstboot-pending.flag', index)
assert not any(line.lstrip().lower().startswith(('fc ', 'comp ')) for line in guest.splitlines())
print('PASS: bundled comparator source wiring and failure guard; WinPE execution unproven')
