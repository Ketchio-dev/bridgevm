"""Five captures, exact prelaunch state transitions, and unchanged tiny files."""
import os

from app_ui_diagnostic import digest, png_dimensions
from app_ui_host_v2_cleanup import read_json
from app_ui_host_v2_manifest import safe_regular
from app_ui_install_cancel_transcript import PHASES

CAPTURES = ("install-idle", "install-validating", "install-cancelling", "install-cancelled", "install-retry-failed")
FIXTURE_FILES = ("fixture-library/pending/vm.json", "fixture-library/pending/bundle/metadata/hvf-install.json",
    "fixture-library/pending/bundle/repo/helpers/wimlib-imagex",
    "fixture-library/pending/bundle/repo/helpers/bridgevm-catalog-verify")
COUNTERS = dict(plan_builds=1, model_creations=1, install_creations=1, validation_calls=2,
    validation_finished=2, validation_main_thread_calls=0, validation_gate_timeouts=0,
    process_resets=2, process_cancels=1, process_runs=0, pipeline_jobs=0, source_verifications=0,
    media_preparations=0, finalizations=0, completions=0, runtime_creations=0, backend_effects=0, file_jobs=0)


def verify_files(output, records):
    if not isinstance(records, list) or len(records) != len(FIXTURE_FILES):
        raise ValueError("install fixture file map differs")
    seen = set()
    for item in records:
        if not isinstance(item, dict) or set(item) != {"path", "bytes", "sha256"}:
            raise ValueError("install fixture file record differs")
        name = item["path"]
        if not isinstance(name, str) or name not in FIXTURE_FILES or name in seen:
            raise ValueError("install fixture path differs")
        seen.add(name)
        path = safe_regular(output / name, 16_384)
        info = path.stat()
        if (info.st_uid != os.geteuid() or info.st_nlink != 1 or type(item["bytes"]) is not int
                or item["bytes"] != info.st_size or item["sha256"] != digest(path)):
            raise ValueError("install fixture bytes changed")
    expected = {output / name for name in FIXTURE_FILES}
    for path in tuple(expected):
        expected.update(parent for parent in path.parents if output in parent.parents)
    actual = set()
    pending = [output / "fixture-library"]
    while pending:
        path = pending.pop()
        if path not in expected or path.is_symlink() or path != path.resolve(strict=True) or path.lstat().st_uid != os.geteuid():
            raise ValueError("unexpected install fixture entry or alias")
        actual.add(path)
        if path.is_dir():
            pending.extend(path.iterdir())
    if actual != expected:
        raise ValueError("install fixture inventory differs")


def verify_observations(output, completed_reads):
    path = output / "install-cancel-observations.json"
    value = read_json(path, {"schema_version", "kind", "fixture_data", "guest_behavior_proof",
        "screenshots", "checkpoints", "counters", "fixture_files", "failure"})
    if (type(value["schema_version"]) is not int or value["schema_version"] != 1
            or value["kind"] != "native-app-ui-install-cancel-observations"
            or value["fixture_data"] is not True or value["guest_behavior_proof"] is not False
            or value["failure"] is not None or not isinstance(value["counters"], dict)
            or set(value["counters"]) != set(COUNTERS)
            or any(type(value["counters"][key]) is not int or value["counters"][key] != count
                   for key, count in COUNTERS.items())):
        raise ValueError("install scenario failed or forbidden domain work was attempted")
    captures = value["screenshots"]
    if not isinstance(captures, list) or len(captures) != 5:
        raise ValueError("install screenshot set is incomplete")
    for name, item in zip(CAPTURES, captures):
        if (not isinstance(item, dict) or set(item) != {"name", "file", "sha256", "width", "height"}
                or item["name"] != name or item["file"] != name + ".png"
                or any(type(item[key]) is not int for key in ("width", "height"))):
            raise ValueError("install screenshot schema differs")
        capture = safe_regular(output / item["file"], 32 * 1024 * 1024)
        if png_dimensions(capture) != (item["width"], item["height"]) or digest(capture) != item["sha256"]:
            raise ValueError("install screenshot digest or dimensions differ")
    checkpoints = value["checkpoints"]
    if not isinstance(checkpoints, list) or len(checkpoints) != 5:
        raise ValueError("install state checkpoint set is incomplete")
    previous = 0
    for index, (phase, item) in enumerate(zip(PHASES, checkpoints)):
        expected = dict(phase=phase, stage=("idle", "validating", "cancelling", "cancelled", "failed")[index],
            is_running=index in (1, 2), can_cancel=index == 1, validator_calls=(0, 1, 1, 1, 2)[index],
            validator_finished=(0, 0, 0, 1, 2)[index], process_resets=(0, 1, 1, 1, 2)[index],
            process_cancels=int(index >= 2), fixture_preserved=True)
        if (not isinstance(item, dict) or set(item) != {*expected, "read_sequence"}
                or any(type(item[key]) is not type(value) or item[key] != value for key, value in expected.items())
                or type(item["read_sequence"]) is not int or item["read_sequence"] <= previous
                or completed_reads.get(item["read_sequence"]) != phase):
            raise ValueError("install checkpoint lacks exact state and AX readback")
        previous = item["read_sequence"]
    verify_files(output, value["fixture_files"])
    return digest(path)
