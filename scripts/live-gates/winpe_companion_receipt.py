"""D4 publication remains non-promoting, including complete diagnostic runs."""
import json
from pathlib import Path
import re
import sys

from winpe_companion_contract import TIER, FLAGS, SAFETY, initial, validate

def job_fields(directory):
    fields = {}
    for line in (directory / "job.env").read_text().splitlines():
        key, value = line.split("=", 1)
        if key in fields:
            raise ValueError("duplicate job field")
        fields[key] = value
    return fields


def write(path, data):
    temporary = path.with_name(path.name + ".pending")
    with temporary.open("x") as stream:
        stream.write(json.dumps(data, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def finalize(directory, commit, publish=False):
    job = job_fields(directory)
    if job["commit"] != commit:
        raise ValueError("commit mismatch")
    private = directory / "receipt.json"
    if private.is_symlink():
        raise ValueError("symlink receipt refused")
    data = json.loads(private.read_text()) if private.is_file() else initial(job)
    validate(data, job)
    if (directory / "cancel.requested").exists():
        data["outcome"] = "canceled"
    if not publish:
        write(private, data)
        return
    public = {key: data[key] for key in initial(job)}
    if "execution_exit_code" in data:
        public["execution_exit_code"] = data["execution_exit_code"]
    public["known_confounders"] = ["One no-3D collection; not WinPE boot or product acceptance proof.",
                                   "Matching preexisting guest files do not prove installation."]
    with (directory / "receipt.public.json").open("x") as stream:
        stream.write(json.dumps(public, indent=2) + "\n")


if __name__ == "__main__":
    mode, directory, commit = sys.argv[1:]
    if mode not in ("finalize", "publish"):
        raise ValueError("unknown mode")
    finalize(Path(directory), commit, mode == "publish")
