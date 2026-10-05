"""Persist an attempt before any output or process can exist."""
from d11_fixture_files import record
from d11_fixture_inputs import Inputs
from d11_fixture_queue_inputs import bound, create_output
from d11_fixture_receipt import empty, collect
from d11_fixture_runtime import execute
from t22_pair_queue import require_source


def run(directory, root, commit, manifest, binary):
    if manifest != directory / "input-manifest.tsv" or binary != directory / "hvf_gic_boot_probe":
        raise ValueError("fixture inputs are not queue-owned")
    binding, _ = bound(directory, commit)
    try:
        with Inputs(manifest, commit, binary) as inputs:
            record(directory / "d11-attempt.private.json", binding)
            output = create_output(binding["job_id"])
            execute(directory, root, inputs, output, binary)
            inputs.check()
        require_source(root, commit)
        value = collect(directory, commit)
    except (OSError, ValueError, subprocess.SubprocessError):
        record(directory / "d11-refusal.private.json", {"reason": "admission-or-integrity-refused"})
        value = empty(binding, "cleanup-unproved")
    record(directory / "receipt.json", value)
    return 0 if value["sealed_fixture"] else 1

