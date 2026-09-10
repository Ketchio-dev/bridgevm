#!/usr/bin/env python3
"""Bounded, observation-only state capture; never changes scene eligibility."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def observe(out):
    destination = out / "scene-failure-observed"
    destination.mkdir(parents=True, exist_ok=True)
    result = {"schema_version": "b6.scene-failure-observation.v1",
              "observation_only": True, "criterion_pass": False,
              "freshness_verified": False, "timeout_seconds": 10}
    surface = out / "display.fb.iosurface"
    if not surface.is_file():
        result["status"] = "surface-unavailable"
    else:
        with (destination / "observer.log").open("wb") as log:
            try:
                child = subprocess.run(
                    [sys.executable, str(Path(__file__).with_name("observe-active-iosurface.py")),
                     "--iosurface", str(surface), "--out", str(destination)],
                    stdout=log, stderr=subprocess.STDOUT, timeout=10, check=False)
                result["returncode"] = child.returncode
                result["status"] = "observer-returned" if child.returncode == 0 else "observer-failed"
            except subprocess.TimeoutExpired:
                result["status"] = "observer-timeout"
            except OSError as error:
                result["status"] = "observer-unavailable"
                result["errno"] = error.errno
    (destination / "observation.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    observe(parser.parse_args().out)
