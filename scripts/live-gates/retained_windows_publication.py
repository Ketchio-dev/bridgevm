"""Publish without replacement and clean only this attempt's owned directory."""
from __future__ import annotations

import os
from pathlib import Path
import uuid
from retained_windows_cleanup import clear_directory
from retained_windows_identity import directory_identity
from retained_windows_mutation import open_owned_directory_for_cleanup, rename_exclusive


class OwnedRetention:
    def __init__(self, staging: Path):
        self.path = staging
        self.last_cleanup_error = None
        self.identity = directory_identity(staging)
        if self.identity is None:
            raise ValueError("retention staging is not an owned directory")

    def publish(self, destination: Path) -> None:
        if directory_identity(self.path) != self.identity:
            raise ValueError("retention staging was replaced before publication")
        rename_exclusive(self.path, destination)
        self.path = destination
        if directory_identity(self.path) != self.identity:
            raise ValueError("retention directory changed during publication")

    def cleanup(self) -> bool:
        descriptor = None
        self.last_cleanup_error = None
        original = self.path
        step = "admit owned directory"
        try:
            if directory_identity(original) != self.identity:
                return False
            step = "unlock owned directory"
            descriptor = open_owned_directory_for_cleanup(original, self.identity)
            quarantine = original.with_name(f".{original.name}.cleanup-{uuid.uuid4().hex}")
            step = "quarantine owned directory"
            rename_exclusive(original, quarantine)
            self.path = quarantine
            if directory_identity(quarantine) != self.identity:
                # A name changed after admission. Restore it only when its old
                # name is still absent; otherwise preserve both entries.
                step = "restore replaced quarantine"
                rename_exclusive(quarantine, original)
                self.path = original
                return False
            step = "clear owned directory"
            clear_directory(descriptor)
            if directory_identity(self.path) != self.identity:
                return False
            step = "remove owned directory"
            os.rmdir(self.path)
            return True
        except OSError as error:
            self.last_cleanup_error = f"{step}: {type(error).__name__}: {error}"
            return False
        finally:
            if descriptor is not None:
                os.close(descriptor)
