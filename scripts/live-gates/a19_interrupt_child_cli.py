"""Command-line parsing for the unchanged declared staged-read observer."""
import argparse
import json
from pathlib import Path

from a19_interrupt_observation import private_new
from a19_interrupt_stop_points import STAGED_RESTORE, STOP_POINTS


def main(run) -> int:
    parser = argparse.ArgumentParser(description="Kill only one owned snapshot helper at a declared staged-read stop point.")
    parser.add_argument("helper", type=Path)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("disk", type=Path)
    parser.add_argument("vars", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--deadline", type=int, default=300)
    parser.add_argument("--stop-point", choices=sorted(STOP_POINTS), default=STAGED_RESTORE.name)
    args = parser.parse_args()
    if not 1 <= args.deadline <= 300:
        parser.error("deadline must be 1..300 seconds")
    result = run(args.helper, args.snapshot, args.disk, args.vars,
                 args.output, args.deadline, args.stop_point)
    result_path = args.output / "interrupt-observation.json"
    with private_new(result_path, "w") as out:
        json.dump(result, out, sort_keys=True)
        out.write("\n")
    return 0
