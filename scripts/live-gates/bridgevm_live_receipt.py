"""Route receipts before importing any repository Python helper."""
import os
from pathlib import Path
import sys

if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: bridgevm_live_receipt.py JOB_DIR JOB_ID")
    here = Path(__file__).resolve().parent
    env = {"HOME": os.environ["HOME"], "PATH": "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"}
    os.execve(sys.executable, [sys.executable, "-I", "-B", "-s", "-E",
        str(here / "bridgevm_live_receipt_route.py"), *sys.argv[1:]], env)
