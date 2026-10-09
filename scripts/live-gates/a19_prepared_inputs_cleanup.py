"""T22 cleanup owns only the directory allocated by this preparation attempt."""
from __future__ import annotations

import os
from pathlib import Path

from retained_windows_publication import OwnedRetention


class PreparedInputsCleanup:
    def __init__(self, path: Path):
        self.path = path
        self.owned: OwnedRetention | None = None

    def allocated(self, path: Path) -> None:
        # Called just after mkdir, not after preparation has finished cloning.
        if path != self.path:
            raise ValueError("prepared-input allocation path differs")
        self.owned = OwnedRetention(path)

    def cleanup(self, live: Path, children_stopped: bool) -> bool:
        try:
            if not children_stopped or os.path.lexists(live):
                return False
            if self.owned is not None and not self.owned.cleanup():
                return False
            return not os.path.lexists(self.path) and not os.path.lexists(live)
        except OSError:
            return False
