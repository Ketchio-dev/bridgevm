"""Bind authentication fields and stamp hashes to one stable bounded JSON read."""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
from pathlib import Path

from native_snapshot_restore_seal import read_bounded_regular
from product_e2e_identity import is_sha256, unique


@dataclass(frozen=True)
class JsonSnapshot:
    path: Path
    data: bytes
    value: object
    sha256: str

    @classmethod
    def read(cls, path: Path) -> JsonSnapshot:
        data = read_bounded_regular(path, 1024 * 1024)
        value = json.loads(data.decode("utf-8"), object_pairs_hook=unique)
        return cls(path, data, value, hashlib.sha256(data).hexdigest())

    def require_seal(self, expected: str) -> None:
        if not is_sha256(expected):
            raise ValueError("prelaunch request SHA-256 is invalid")
        if self.sha256 != expected:
            raise ValueError("request differs from its prelaunch SHA-256")

    def require_unchanged(self) -> None:
        if read_bounded_regular(self.path, 1024 * 1024) != self.data:
            raise ValueError(f"authenticated JSON changed before stamping: {self.path.name}")


def unchanged(*documents: JsonSnapshot) -> None:
    for document in documents:
        document.require_unchanged()
