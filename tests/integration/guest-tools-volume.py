#!/usr/bin/env python3
"""Exercise the real builder; optional output parent can be an owners-disabled volume."""
import pathlib
import plistlib
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='bv-tools-output-', dir=sys.argv[1] if len(sys.argv) > 1 else None) as output_dir:
    output = pathlib.Path(output_dir) / 'tools.raw'
    result = subprocess.run(['bash', str(root/'scripts/build-guest-tools-volume.sh'), str(root/'scripts/win-assets'), str(output)], capture_output=True, text=True)
    assert result.returncode == 0, 'tools image must build: ' + result.stderr
    with tempfile.TemporaryDirectory(prefix='bv-tools-inspect-', dir='/private/tmp') as mount:
        mounted = subprocess.check_output(['hdiutil', 'attach', '-imagekey', 'diskimage-class=CRawDiskImage', '-readonly', '-nobrowse', '-mountpoint', mount, '-plist', str(output)])
        device = plistlib.loads(mounted)['system-entities'][0]['dev-entry']
        try:
            names = sorted(p.name for p in pathlib.Path(mount).iterdir())
            expected = sorted(['bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1', 'bvagent-update.ps1', 'install.cmd', 'bvagent-install.ps1'])
            assert names == expected, 'only intended files, no AppleDouble metadata: ' + repr(names)
            for name in names:
                assert (pathlib.Path(mount)/name).read_bytes() == (root/'scripts/win-assets'/name).read_bytes(), name
            print('PASS: exact 11 names and bytes; no filler; image_bytes=' + str(output.stat().st_size))
        finally:
            subprocess.run(['hdiutil', 'detach', device, '-quiet'], check=True)
