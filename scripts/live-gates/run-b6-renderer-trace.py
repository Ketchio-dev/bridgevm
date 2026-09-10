#!/usr/bin/env python3
"""Physical renderer diagnosis; never a performance or release gate."""
import importlib.util
import os
import pathlib
import sys

from b6_renderer_trace import CONFOUNDERS, POLICY, POLICY_HASH, TIER, finish_trace

path = pathlib.Path(__file__).with_name("run-b6-cell-observation.py")
spec = importlib.util.spec_from_file_location("b6_trace_cell_core", path)
core = importlib.util.module_from_spec(spec)
spec.loader.exec_module(core)


def trace_receipt(job_id, commit):
    value = core.receipt(job_id, commit)
    value.update(tier=TIER, gate_id="b6-renderer-trace",
                 environment_policy_sha256=POLICY_HASH, known_confounders=list(CONFOUNDERS))
    return value


def run_trace(args):
    previous = os.environ.get("VREND_DEBUG")
    os.environ["VREND_DEBUG"] = POLICY["VREND_DEBUG"]
    try:
        return core.run(args, receipt_factory=trace_receipt,
                        complete=lambda value, out: finish_trace(core, value, out))
    finally:
        if previous is None:
            os.environ.pop("VREND_DEBUG", None)
        else:
            os.environ["VREND_DEBUG"] = previous


if __name__ == "__main__":
    sys.exit(core.main(run_trace))
