"""T21 removes only the prepared tree allocated by this attempt."""
from __future__ import annotations

from pathlib import Path

from a19_quota_seal import present
from retained_windows_publication import OwnedRetention

OWNED = ("prepared-inputs", "quota-refusal.snapshot", ".quota-refusal.snapshot.staging",
         "quota-boundary.snapshot", ".quota-boundary.snapshot.staging",
         ".bridgevm-snapshot-parent-lease")


class QuotaCleanup:
    def __init__(self, output: Path):
        self.output = output
        self.prepared = output / "prepared-inputs"
        self.owned: OwnedRetention | None = None

    def allocated(self, path: Path) -> None:
        # prepare calls this immediately after its exclusive mkdir, before clones.
        if path != self.prepared or self.owned is not None:
            raise ValueError("unexpected quota prepared-input allocation")
        self.owned = OwnedRetention(path)

    def cleanup(self) -> bool:
        try:
            # Helpers write inside this allocated tree. Never adopt loose output
            # names based on absence before invocation or presence afterwards.
            if self.owned is not None and not self.owned.cleanup():
                return False
            return not any(present(self.output / name) for name in OWNED)
        except OSError:
            return False
