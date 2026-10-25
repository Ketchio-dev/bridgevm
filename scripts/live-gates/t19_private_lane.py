"""Private T19 lane-result fields: the shared ones (product_e2e_private_lane.py) and the opt-in
BitLocker restart check's evidence (docs/history/windows-hvf/bitlocker-restart-check-20261023.md).
Like the shared ones, it never decides a lane and never reaches the public receipt; the T17 writer
keeps the shared set, so a T17 lane result carrying it is refused."""
from product_e2e_private_lane import KEYS as SHARED

KEYS = SHARED | frozenset({"bitlocker_restart_check"})
