"""Export a lane's vTPM key for a T19 handoff without letting product output reach the tier log.

The product's messages can name private paths, and live-gate logs carry only
booleans, sizes, hashes and fixed tokens. The export's stdout and stderr are
captured with a bound and a timeout, as t17_lane_key_forget.py does, and never
echoed; every failure is a fixed refusal naming only the exit status or errno.
"""
from __future__ import annotations

import errno
from pathlib import Path
import subprocess

TIMEOUT_SECONDS = 120
OUTPUT_LIMIT = 65_536


def export_vtpm(executable: Path, slug: str, state: Path, package: Path, code: Path) -> None:
    arguments = ["--stable-vm-id", slug, "--state-dir", str(state), "--package", str(package),
                 "--recovery-code-file", str(code)]
    try:
        completed = subprocess.run([str(executable), "--vtpm-lifecycle", "export", *arguments],
                                   stdin=subprocess.DEVNULL, capture_output=True, timeout=TIMEOUT_SECONDS, check=False)
    except subprocess.TimeoutExpired:
        raise ValueError(f"packaged vTPM export did not finish within {TIMEOUT_SECONDS} s") from None
    except OSError as error:
        raise ValueError(f"packaged vTPM export could not start ({errno.errorcode.get(error.errno, 'no errno')})") from None
    if len(completed.stdout) > OUTPUT_LIMIT or len(completed.stderr) > OUTPUT_LIMIT:
        raise ValueError(f"packaged vTPM export output exceeds {OUTPUT_LIMIT} bytes")
    if completed.returncode != 0:
        raise ValueError(f"packaged vTPM export failed with exit status {completed.returncode}")
