"""Path-free, bounded evidence checks for diagnostic B6 cell observations."""
import json
import pathlib
import re

from b6_cell_inputs import file_hash, small_bytes
from b6_renderer_trace import log_evidence
from b6_frame_times import NO_CLAIM, summarize


def validate_capture(out, config):
    capture = pathlib.Path(out) / "capture"
    lines = small_bytes(capture / "summary.txt", 8192).decode("utf-8").splitlines()
    fields = dict(line.split("=", 1) for line in lines)
    if len(fields) != len(lines):
        raise ValueError("duplicate summary field")
    expected = dict(width=str(config["width"]), height=str(config["height"]),
                    logpixels=str(config["logpixels"]), f1_driver_load="pass",
                    f2_resize="pass", scale_set="pass", runs_complete="true")
    if any(fields.get(key) != value for key, value in expected.items()):
        raise ValueError("cell summary does not prove the requested observation")
    runs = json.loads(small_bytes(capture / "runs.json", 16384))
    if not isinstance(runs, list) or len(runs) != 3 or any(not isinstance(row, dict) for row in runs):
        raise ValueError("expected three paired scene observations")
    if any(type(row.get("run")) is not int for row in runs) or [row["run"] for row in runs] != [1, 2, 3]:
        raise ValueError("run identities are incomplete or reused")
    hashes = []
    for row in runs:
        for key in ("classic_dpi", "packaged_dpi", "classic_focus", "packaged_focus",
                    "presentmon_status", "packaged_presentmon_status"):
            if row.get(key) != "pass":
                raise ValueError("scene prerequisite failed: " + key)
        for key in ("classic_capture", "packaged_capture", "presentmon_csv", "packaged_presentmon_csv"):
            if row.get(key) != "present":
                raise ValueError("scene artifact absent: " + key)
        for scene in ("classic", "packaged"):
            stem = "presentmon-" + scene + "-run" + str(row["run"])
            report_path = capture / (stem + ".frame-times.json")
            report = json.loads(small_bytes(report_path, 65536))
            if report.get("valid") is not True or any(report.get(key) is not False for key in NO_CLAIM):
                raise ValueError("invalid or claim-bearing diagnostic")
            issued = report.get("host_input_commands")
            if type(issued) is not int or issued < 1:
                raise ValueError("no recorded scene input")
            digest = report.get("guest_reported_sha256", "")
            if not isinstance(digest, str) or not re.fullmatch("[0-9a-f]{64}", digest):
                raise ValueError("missing guest CSV identity")
            parsed = summarize(capture / "share" / (stem + ".csv"), digest)[0]
            if any(report.get(key) != value for key, value in parsed.items()):
                raise ValueError("diagnostic differs from authenticated CSV")
            hashes.append(file_hash(report_path))
    return dict(schema_version=1, **config, run_count=3, frame_report_sha256=hashes, **NO_CLAIM)


def screen_stage_log(out, stage):
    """Authenticate one bounded stage log and refuse renderer startup failures."""
    if stage not in ("scale", "capture"):
        raise ValueError("unknown renderer stage")
    try:
        return log_evidence(pathlib.Path(out) / stage / "run.log")
    except OSError as error:
        raise ValueError(stage + " renderer log unavailable") from error
