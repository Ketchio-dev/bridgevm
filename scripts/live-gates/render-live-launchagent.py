#!/usr/bin/env python3
"""Render explicit non-secret launch settings as plist data, never sed source."""
import re
import runpy
from pathlib import Path
import plistlib
import sys

capacity = runpy.run_path(str(Path(__file__).with_name("live_storage_capacity.py")))
storage_path, threshold = capacity["storage_path"], capacity["threshold"]


def render(template, home, user, worker, logs, queue, work, minimum):
    threshold(minimum); queue = runpy.run_path(str(Path(__file__).with_name("queue-root-path.py")))["root"](queue)
    values = {"__HOME__": home, "__USER__": user, "__WORKER__": worker, "__LOGDIR__": logs,
              "__QUEUE__": str(storage_path(queue).resolve(strict=True)),
              "__WORK__": str(storage_path(work).resolve()), "__MINIMUM__": minimum}
    def replace(value):
        if isinstance(value, str):
            return re.sub("|".join(map(re.escape, values)), lambda match: values[match.group()], value)
        if isinstance(value, list): return [replace(item) for item in value]
        if isinstance(value, dict): return {key: replace(item) for key, item in value.items()}
        return value
    return replace(plistlib.loads(Path(template).read_bytes()))


if __name__ == "__main__":
    try:
        result = render(*sys.argv[1:])
        sys.stdout.buffer.write(plistlib.dumps(result, sort_keys=False))
    except (OSError, ValueError, TypeError, RuntimeError):
        raise SystemExit("launch storage configuration refused")
