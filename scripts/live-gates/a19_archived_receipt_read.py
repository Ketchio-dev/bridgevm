"""Authenticate T21/T22 archive reads without promoting historical receipts."""
from __future__ import annotations

import os
from pathlib import Path

import a19_interrupted_restore_receipt as interrupt
import a19_quota_refusal_receipt as quota
from a19_lifecycle_campaign_read import strict_reader as lifecycle_reader
from native_snapshot_restore_public import load_receipt

OWNED = {
    quota.TIER: ("prepared-inputs", "quota-refusal.snapshot", ".quota-refusal.snapshot.staging",
                 "quota-boundary.snapshot", ".quota-boundary.snapshot.staging",
                 ".bridgevm-snapshot-parent-lease"),
    interrupt.TIER: ("prepared-inputs", "live"),
}


def _read(public: Path, directory: Path, contract) -> dict:
    if public != directory / "receipt.public.json" or directory.parent.name != "done" or directory.parent.is_symlink():
        raise ValueError("T21/T22 receipt is not an archived public result")
    value = contract.validate(load_receipt(public))
    private = contract.validate(load_receipt(directory / "receipt.json"))
    if value != private:
        raise ValueError("T21/T22 public receipt differs from its private original")
    contract.validate_seal(value, directory)
    if any(os.path.lexists(directory / name) for name in OWNED[contract.TIER]):
        raise ValueError("T21/T22 receipt has owned private-media residue")
    return value


def read_quota(public: Path, directory: Path) -> dict:
    return _read(public, directory, quota)


def read_interrupt(public: Path, directory: Path) -> dict:
    return _read(public, directory, interrupt)


def strict_reader(tiers: tuple[object, ...]):
    """Any strict tier hint prevents falling back to the legacy byte stream."""
    for tier, reader in ((quota.TIER, read_quota), (interrupt.TIER, read_interrupt)):
        if tier in tiers:
            return reader
    return lifecycle_reader(tiers)
