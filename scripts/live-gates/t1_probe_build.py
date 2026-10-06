"""Select the executable actually emitted by this legacy Debug Cargo build."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from t1_probe_artifact import command, snapshot

def target():
    value = json.loads(command(['cargo', 'metadata', '--no-deps', '--locked', '--format-version', '1']))
    return str(Path(value['target_directory']) / 'debug/examples/hvf_vtimer_cancel_probe')



def build():
    argv = ['cargo', 'build', '--locked', '-p', 'bridgevm-hvf', '--example', 'hvf_vtimer_cancel_probe', '--message-format=json']
    with tempfile.TemporaryFile() as output:
        result = subprocess.run(argv, stdout=output, check=False)
        output.seek(0);data = output.read(8 * 1024 * 1024 + 1)
    if len(data) > 8 * 1024 * 1024:raise ValueError('T1 Cargo artifact output exceeds bound')
    if result.returncode:
        sys.stderr.buffer.write(data);raise subprocess.CalledProcessError(result.returncode, argv)
    artifacts = []
    for line in data.splitlines():
        value = json.loads(line)
        target = value.get('target', {})
        if value.get('reason') == 'compiler-artifact' and target.get('name') == 'hvf_vtimer_cancel_probe' and target.get('kind') == ['example'] and value.get('executable'):
            artifacts.append(value['executable'])
    if len(artifacts) != 1 or not Path(artifacts[0]).is_absolute():raise ValueError('T1 Cargo executable is missing or ambiguous')
    snapshot(artifacts[0])
    return artifacts[0]
