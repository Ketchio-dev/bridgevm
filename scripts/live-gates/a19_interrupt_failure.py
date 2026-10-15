"""Retain private failed-observation status without changing child ownership."""
import json

from a19_interrupt_observation import private_new


class FailedObservation:
    def __init__(self, output, stage):
        self.output, self.stage, self.child = output, stage, None

    def __enter__(self):
        return self

    def __exit__(self, error_type, error, traceback):
        if error is None or self.child is None:
            return False
        value = {"schema_version": 1, "pass": False,
                 "interruption_stage": self.stage, "helper_pid": self.child.pid,
                 "helper_returncode": self.child.returncode,
                 "helper_returncode_known": self.child.returncode is not None,
                 "status_source": "Popen.returncode", "failure_type": error_type.__name__}
        try:
            with private_new(self.output / "interrupt-helper-failure.private.json", "w") as output:
                json.dump(value, output, sort_keys=True)
                output.write("\n")
        except (OSError, ValueError, TypeError):
            add_note = getattr(BaseException, "add_note", None)
            if add_note is not None:
                add_note(error, "failure receipt could not be retained")
        return False
