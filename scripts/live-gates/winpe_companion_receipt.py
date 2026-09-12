"""D4 publication remains non-promoting, including complete diagnostic runs."""
import json
from pathlib import Path
import re
import sys

TIER = "d4-winpe-companions"
FLAGS = ("pass", "claim_eligible", "criterion_pass", "capability_promotion", "winpe_boot_proven")


def job_fields(directory):
    fields = {}
    for line in (directory / "job.env").read_text().splitlines():
        key, value = line.split("=", 1)
        if key in fields:
            raise ValueError("duplicate job field")
        fields[key] = value
    return fields


def initial(job):
    return {"tier": TIER, **{key: job[key] for key in ("job_id", "commit", "input_manifest_sha256")},
            **{key: False for key in FLAGS}, "outcome": "diagnostic-incomplete",
            "sample_count": 0, "required_run_count": 1, "passes": 0,
            "source_integrity_verified": False, "post_files_match": False,
            "files_changed": False, "pre_post_available": False}


def validate(data, job):
    if job.get("tier") != TIER or data.get("tier") != TIER:
        raise ValueError("invalid diagnostic tier")
    for key, pattern in (("job_id", r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}"),
                         ("commit", r"[0-9a-f]{40}"), ("input_manifest_sha256", r"[0-9a-f]{64}")):
        if data.get(key) != job.get(key) or not re.fullmatch(pattern, data.get(key, "")):
            raise ValueError("invalid sealed identity")
    if any(data.get(key) is not False for key in FLAGS):
        raise ValueError("diagnostic cannot promote a claim")
    for key, allowed in (("passes", (0,)), ("sample_count", (0, 1)), ("required_run_count", (1,))):
        if type(data.get(key)) is not int or data[key] not in allowed:
            raise ValueError("invalid diagnostic count")
    for key in ("source_integrity_verified", "post_files_match", "files_changed", "pre_post_available"):
        if type(data.get(key)) is not bool:
            raise ValueError("invalid observation flag")
    if data.get("outcome") not in ("diagnostic-incomplete", "diagnostic-complete", "canceled"):
        raise ValueError("invalid diagnostic outcome")
    if data["outcome"] == "diagnostic-complete":
        if data["sample_count"] != 1 or not data["source_integrity_verified"]:
            raise ValueError("incomplete collection")
        if type(data.get("execution_exit_code")) is not int or not -255 <= data["execution_exit_code"] <= 255:
            raise ValueError("invalid process result")


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
