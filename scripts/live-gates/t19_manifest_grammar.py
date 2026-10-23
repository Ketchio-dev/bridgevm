"""The T19 input manifest's rows; windows-import-product-e2e-manifest.py authenticates what they name.

One `campaign_mode` row and one row per sealed asset. One optional row asks for the opt-in BitLocker
restart check (docs/history/windows-hvf/bitlocker-restart-check-20261023.md): exactly
`bitlocker_restart_check<TAB>tpm-only`, at most once and only in a pilot manifest. Any other row
refuses the manifest, and without that row the verified inputs gain nothing."""
from __future__ import annotations
import os, re
from pathlib import Path

ASSETS = ("app_bundle", "app_executable", "product_helper", "runner",
          "source_disk", "source_vars", "source_vtpm", "source_vtpm_package", "source_vtpm_code")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
RELATIONS = {
    "app_executable": "Contents/MacOS/BridgeVMControl",
    "product_helper": "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E",
    "runner": "Contents/Resources/target/release/hvf-runner",
}
SWITCH, SWITCH_VALUE = "bitlocker_restart_check", "tpm-only"

class ManifestError(ValueError):
    pass

def parse(path: Path) -> tuple[str, dict[str, tuple[Path, str]], dict[str, str]]:
    """The mode, the sealed assets and the verified-input fields the switch adds."""
    if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= 16 * 1024:
        raise ManifestError("manifest is missing, unsafe, empty, or oversized")
    mode: str | None = None; assets: dict[str, tuple[Path, str]] = {}
    switch: list[str] = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        if fields[0] == "campaign_mode" and len(fields) == 2 and mode is None:
            mode = fields[1]; continue
        if fields[0] == SWITCH:
            switch.append("\t".join(fields[1:])); continue
        if len(fields) != 3 or fields[0] not in ASSETS or fields[0] in assets:
            raise ManifestError(f"malformed, duplicate, or unknown row {number}")
        raw, expected = fields[1], fields[2]
        if not raw.startswith("/") or os.path.normpath(raw) != raw or not SHA256.fullmatch(expected):
            raise ManifestError(f"invalid path or digest in row {number}")
        assets[fields[0]] = Path(raw), expected
    if mode not in ("pilot", "release") or set(assets) != set(ASSETS):
        raise ManifestError("manifest lacks its one exact mode and asset set")
    if switch and (switch != [SWITCH_VALUE] or mode != "pilot"):
        raise ManifestError("the BitLocker restart check needs one tpm-only row in a pilot manifest")
    bundle = assets["app_bundle"][0]
    for key, relative in RELATIONS.items():
        if assets[key][0] != bundle / relative:
            raise ManifestError(f"{key} is outside its fixed app location")
    return mode, assets, {SWITCH: SWITCH_VALUE} if switch else {}
