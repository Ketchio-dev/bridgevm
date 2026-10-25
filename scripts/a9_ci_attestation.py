"""Validate one recorded GitHub-hosted CI run for an A9 release claim, on the JSON alone.

scripts/fetch-a9-ci-attestation.py is the only A9 claim code that talks to GitHub; it
writes these attestations. Nothing here uses the network. Two workflows are attestable:

- hosted-full, 'Hosted full project check': workflow_dispatch only. The run's head_sha
  is the dispatched ref, but the job checks out inputs.source_sha, so the attestation
  also records the job's own `checked_source_sha=` log line. Both must be the commit.
- security-quality, 'Security and quality': only push and workflow_dispatch runs check
  out head_sha itself. A pull_request run tests the merge ref, so it is refused.

An attestation always describes a completed run. Whether it was green is a separate
question (succeeded) that the combined claim rule asks.
"""
from __future__ import annotations

from datetime import datetime
import re

SCHEMA = "bridgevm.a9-ci-run-attestation.v1"
REPOSITORY = "Ketchio-dev/bridgevm"
KINDS = {
    "hosted-full": {
        "workflow_name": "Hosted full project check",
        "workflow_path": ".github/workflows/hosted-full-project-check.yml",
        "events": ("workflow_dispatch",),
        "checked_source": True,
    },
    "security-quality": {
        "workflow_name": "Security and quality",
        "workflow_path": ".github/workflows/security-quality.yml",
        "events": ("push", "workflow_dispatch"),
        "checked_source": False,
    },
}
FIELDS = frozenset({
    "schema_version", "repository", "workflow_name", "workflow_path", "run_id", "run_attempt",
    "head_sha", "checked_source_sha", "event", "status", "conclusion", "html_url", "fetched_at",
})
CONCLUSIONS = frozenset({"success", "failure", "cancelled", "timed_out", "skipped", "neutral",
                         "action_required", "stale", "startup_failure"})
COMMIT = re.compile(r"[0-9a-f]{40}\Z")
RUN_ID = re.compile(r"[1-9][0-9]{0,19}\Z")
TIMESTAMP = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z\Z")


class AttestationError(ValueError):
    """The CI attestation is malformed or does not attest the claimed commit."""


def _text(value: object, pattern: re.Pattern[str]) -> bool:
    return type(value) is str and pattern.fullmatch(value) is not None


def validate(value: object, *, kind: str, commit: str) -> dict:
    if kind not in KINDS:
        raise AttestationError(f"unknown CI attestation kind: {kind!r}")
    if not _text(commit, COMMIT):
        raise AttestationError("claimed commit must be a full lowercase commit")
    expected = KINDS[kind]
    label = f"{kind} CI attestation"
    if not isinstance(value, dict) or frozenset(value) != FIELDS:
        raise AttestationError(f"{label} has missing or unknown fields")
    fixed = {"schema_version": SCHEMA, "repository": REPOSITORY,
             "workflow_name": expected["workflow_name"], "workflow_path": expected["workflow_path"]}
    for key, item in fixed.items():
        if type(value[key]) is not str or value[key] != item:
            raise AttestationError(f"{label} {key} is {value[key]!r}, not {item!r}")
    if not _text(value["run_id"], RUN_ID):
        raise AttestationError(f"{label} run_id must be a decimal GitHub run id")
    if type(value["run_attempt"]) is not int or value["run_attempt"] < 1:
        raise AttestationError(f"{label} run_attempt must be an integer >= 1")
    if not _text(value["head_sha"], COMMIT) or value["head_sha"] != commit:
        raise AttestationError(f"{label} head_sha {value['head_sha']!r} is not the claimed commit {commit}")
    checked = value["checked_source_sha"]
    if expected["checked_source"]:
        if not _text(checked, COMMIT) or checked != commit:
            raise AttestationError(f"{label} checked_source_sha {checked!r} is not the claimed commit {commit}")
    elif checked != "absent":
        raise AttestationError(f"{label} checked_source_sha must be 'absent'; this workflow checks out head_sha")
    if type(value["event"]) is not str or value["event"] not in expected["events"]:
        raise AttestationError(f"{label} event {value['event']!r} is not one of {', '.join(expected['events'])}")
    if value["status"] != "completed":
        raise AttestationError(f"{label} status {value['status']!r} is not completed")
    if type(value["conclusion"]) is not str or value["conclusion"] not in CONCLUSIONS:
        raise AttestationError(f"{label} conclusion {value['conclusion']!r} is not recognized")
    if value["html_url"] != f"https://github.com/{REPOSITORY}/actions/runs/{value['run_id']}":
        raise AttestationError(f"{label} html_url does not name run {value['run_id']}")
    if not _text(value["fetched_at"], TIMESTAMP):
        raise AttestationError(f"{label} fetched_at must be a UTC second-resolution timestamp")
    try:
        datetime.strptime(value["fetched_at"], "%Y-%m-%dT%H:%M:%SZ")
    except ValueError as error:
        raise AttestationError(f"{label} fetched_at is not a real time") from error
    return value


def succeeded(value: dict) -> bool:
    return value["status"] == "completed" and value["conclusion"] == "success"
