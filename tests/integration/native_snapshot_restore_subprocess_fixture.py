"""Run the real T20 entry point with only public host metadata mocked."""
import subprocess
import sys

from native_snapshot_restore_cleanup_fixture import SCRIPT

HOST_METADATA_RUNNER = """
import platform, runpy, subprocess, sys
from pathlib import Path
from unittest.mock import patch
sys.argv = sys.argv[1:]
sys.path.insert(0, str(Path(sys.argv[0]).parent))
actual_output = subprocess.check_output
def host_output(args, **options):
    if args == ["sysctl", "-n", "hw.model"]:
        return "Mac16,9\\n"
    return actual_output(args, **options)
with patch.object(subprocess, "check_output", side_effect=host_output), \\
        patch.object(platform, "mac_ver", return_value=("15.7.9", (), "")):
    runpy.run_path(sys.argv[0], run_name="__main__")
"""


def run_failed_seal(output):
    return subprocess.run([sys.executable, "-c", HOST_METADATA_RUNNER, str(SCRIPT),
                           str(output), output.name, str(output.parent / "missing-manifest"),
                           str(output.parent / "missing-binary")],
                          capture_output=True, text=True, timeout=20)
