"""Bounded cleanup of a process group created and owned by this diagnostic."""
import json

from guest_input_live_inputs import digest
from guest_input_group_liveness import group_alive
from guest_input_owned_group import stop


def finalize(receipt, process, paths, hashes, clones, work, *, spawn_attempted=False):
    receipt["complete"] = False
    try:
        receipt["cleanup_complete"] = False if spawn_attempted and process is None else stop(process)
    except (OSError, ValueError) as error:
        receipt["cleanup_complete"] = False
        receipt["cleanup_failure_type"] = type(error).__name__
    if receipt["cleanup_complete"]:
        try:
            receipt["source_integrity"] = all(digest(path) == hashes[name] for name, path in paths.items())
            receipt["output_hashes"] = {name: digest(path) for name, path in clones.items()}
            for path in clones.values():
                path.chmod(0o400)
            receipt["complete"] = True
        except (OSError, ValueError) as error:
            receipt["integrity_failure_type"] = type(error).__name__
    # Do not hash or chmod disks that a surviving process may still be writing.
    (work / "receipt.json").write_text(json.dumps(receipt, sort_keys=True) + "\n")
