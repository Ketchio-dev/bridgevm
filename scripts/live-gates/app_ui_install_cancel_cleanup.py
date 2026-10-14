"""Independent owned dual-app exit remains mandatory, even after UI refusal."""
from pathlib import Path
import sys

from app_ui_diagnostic import digest
from app_ui_host_v2_cleanup import load_job, read_json, verify_launcher_exit
from app_ui_install_cancel_manifest import TIER, LAUNCHER_KIND, LAUNCHER_FILE, parse_manifest


def verify_exit(private, fields):
    return verify_launcher_exit(private / LAUNCHER_FILE, private, fields, kind=LAUNCHER_KIND)


def verify_job_cleanup(output, commit):
    output = Path(output)
    manifest = output / "input-manifest.tsv"
    fields = parse_manifest(manifest, commit)
    expected = fields["binary"][1]
    job_id = load_job(output, commit, manifest, expected, tier=TIER)
    receipt = read_json(output / "receipt.json", ("tier", "commit", "job_id", "binary_hash",
        "input_manifest_sha256", "helper_sha256", "run_count", "claim_eligible", "criterion_pass",
        "capability_promotion"), ("boots_attempted", "sample_count", "passes", "failures", "pass",
        "worker_cleanup_verified", "outcome", "failure_code", "result_sha256", "driver_result_sha256"))
    if (receipt["tier"] != TIER or receipt["commit"] != commit or receipt["job_id"] != job_id
            or receipt["binary_hash"] != expected or receipt["input_manifest_sha256"] != digest(manifest)
            or receipt["helper_sha256"] != fields["launcher"][1]
            or type(receipt["run_count"]) is not int or receipt["run_count"] not in (0, 1)
            or any(receipt[k] is not False for k in ("claim_eligible", "criterion_pass", "capability_promotion"))):
        raise ValueError("install cancellation cleanup receipt is not bound to this job")
    private = output / "app-ui-private"
    if receipt["run_count"] == 0 and not private.exists() and not private.is_symlink():
        return
    if receipt["run_count"] != 1 or not private.is_dir() or private.is_symlink():
        raise ValueError("install cancellation prelaunch cleanup cannot be established")
    verify_exit(private, fields)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 3:
            raise ValueError("expected JOB_DIRECTORY COMMIT")
        verify_job_cleanup(*sys.argv[1:])
    except (OSError, ValueError) as error:
        print(f"install cancellation cleanup unresolved: {error}", file=sys.stderr)
        sys.exit(126)
