"""Adversarial outputs for the synthetic installed-media import helper."""
import json
from pathlib import Path


def mutate(request: dict, result: dict, request_path: str) -> None:
    if "bad-hash" in request["job_id"]:
        result["final_disk_sha256"] = "0" * 64
    if "request-tamper" in request["job_id"]:
        rewritten = {**request, "source_vtpm_code_path": "/private/tmp/unrequested-code.txt"}
        Path(request_path).write_text(json.dumps(rewritten))
    if "request-append" in request["job_id"]:
        with Path(request_path).open("a") as output:
            output.write(" ")
