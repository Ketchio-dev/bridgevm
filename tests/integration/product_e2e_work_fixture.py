"""Owned allocation fixtures shared by request/identity tests."""
from pathlib import Path
import sys

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts/live-gates"
sys.path.insert(0, str(SCRIPTS))
import product_e2e_work as WORK


def allocate(parent, job="integrity-fixture", kind="import-e2e"):
    parent = Path(parent).resolve()
    parent.chmod(0o700)
    work = parent / f"bridgevm-{kind}-{job}.Ab12Cd"
    work.mkdir(mode=0o700)
    lane = work / "lane-1"
    lane.mkdir(mode=0o700)
    return lane
