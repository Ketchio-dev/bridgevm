"""Queue-owned Swift driver dispatch for the actual guest input diagnostic."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

from guest_input_controller import Controller
from guest_input_profiles import PROFILE, PRODUCTION_PROFILE, asset_names
from guest_input_protocol import regular_bytes


def seal(source, directory):
    from guest_input_live_inputs import digest
    value = json.loads(regular_bytes(source, 16384))
    names = asset_names(value["profile"])
    if set(value["assets"]) != names:
        raise ValueError("profile asset set mismatch")
    if "driver" not in names:
        return
    entry = value["assets"]["driver"]
    path = Path(entry["path"])
    if not path.is_absolute() or str(path.resolve()) != entry["path"] or path.is_symlink():
        raise ValueError("driver path must be canonical")
    if digest(path) != entry["sha256"]:
        raise ValueError("driver source hash mismatch")
    target = directory / "production-input-driver"
    with path.open("rb") as source_file, target.open("xb") as output:
        shutil.copyfileobj(source_file, output)
    target.chmod(0o500)
    if digest(target) != entry["sha256"]:
        raise ValueError("driver copy hash mismatch")


def bind(directory, value):
    from guest_input_live_inputs import digest
    names = asset_names(value["profile"])
    if set(value["assets"]) != names:
        raise ValueError("profile asset set mismatch")
    if "driver" in names:
        target = directory / "production-input-driver"
        if target.is_symlink() or digest(target) != value["assets"]["driver"]["sha256"]:
            raise ValueError("queue driver seal mismatch")
        value["assets"]["driver"]["path"] = str(target.resolve())


def checked_report(value):
    expected = {"schema": "bridgevm.production-input-driver.v1", "claim_eligible": False,
                "production_ui_proven": False, "guest_application_proven": False,
                "driver_receipts_observed": True, "sent": 4, "inserted": 4, "failure": "none"}
    if not isinstance(value, dict) or set(value) != set(expected):
        raise ValueError("invalid production driver report fields")
    if any(type(value[k]) is not type(v) or value[k] != v for k, v in expected.items()):
        raise ValueError("production driver receipt observation failed")


class ProductionController(Controller):
    def __init__(self, control, log, share, driver):
        super().__init__(control, log, share)
        self.driver, self.driver_observed = driver, False

    def dispatch_inputs(self, x, y):
        result = subprocess.run([str(self.driver), str(self.control), str(self.log), str(x), str(y)],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40)
        if result.returncode != 0 or len(result.stdout) > 16384 or result.stderr:
            raise ValueError("production input driver failed")
        value = json.loads(result.stdout)
        checked_report(value)
        with (self.share.parent / "production-driver.json").open("x") as report:
            json.dump(value, report, sort_keys=True)
        self.driver_observed = True


def make_controller(control, log, share, paths):
    if "driver" in paths:
        return ProductionController(control, log, share, paths["driver"])
    return Controller(control, log, share)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("requires manifest and queue staging directory")
    seal(Path(sys.argv[1]), Path(sys.argv[2]))
