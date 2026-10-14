#!/usr/bin/env python3
"""T12 submission and run preflight, without compilation or signing mutations."""
from pathlib import Path
import subprocess
import sys

from t12_input_manifest import Inputs
from t12_input_seal import seal, submitted

REPO = Path(__file__).resolve().parents[2]


def main(argv):
    mode = argv[0]
    if mode == "validate" and len(argv) == 3:
        with Inputs(Path(argv[1]), argv[2], REPO): pass
    elif mode == "seal" and len(argv) == 4:
        seal(Path(argv[1]), argv[2], Path(argv[3]), REPO)
    elif mode == "run" and len(argv) == 6:
        directory, commit, name, manifest, binary = Path(argv[1]), argv[2], argv[3], Path(argv[4]), Path(argv[5])
        value = submitted(directory, commit, name, manifest, binary, REPO)
        subprocess.run(["codesign", "--verify", "--strict", str(binary)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        print(" ".join(value[k] for k in ("input_manifest_sha256", "binary_sha256", "firmware_sha256",
                                         "image_sha256", "initial_vars_sha256", "rust_toolchain")))
    else: raise ValueError("invalid T12 input operation")


if __name__ == "__main__":
    try: main(sys.argv[1:])
    except (OSError, ValueError, UnicodeError, IndexError, subprocess.SubprocessError) as error:
        print(f"T12 sealed input refused: {error}", file=sys.stderr)
        sys.exit(1)
