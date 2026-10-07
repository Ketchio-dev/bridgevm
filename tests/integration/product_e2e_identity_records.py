"""Synthetic result and stamp identities, separate from filesystem fixtures."""

def lane(writer, JOB, COMMIT, NONCE) -> dict:
    value = {"schema_version": writer.LANE_SCHEMA, "job_id": JOB, "commit": COMMIT,
             "campaign_mode": "pilot", "lane": 1, "nonce": NONCE, "three_d_injection": False,
             "ui_frontend_automated": True, "failure_code": "none", "failure_detail": "",
             "cleanup_verified": True, **dict.fromkeys(writer.LANE_STAGES, True),
             **dict.fromkeys(writer.LANE_HASHES, "c" * 64)}
    if writer.LANE_SCHEMA == "bridgevm.windows-hvf-3d-off-product-e2e-lane.v2":
        value["installer_source_path"] = "/private/tmp/synthetic-source.raw"
    return value


def stamp(writer, result, JOB, COMMIT, NONCE) -> dict:
    schema = ("bridgevm.windows-hvf-3d-off-product-e2e-host-stamp.v1" if writer.LANE_SCHEMA == "bridgevm.windows-hvf-3d-off-product-e2e-lane.v2"
              else "bridgevm.windows-hvf-import-product-e2e-host-stamp.v1")
    return {"schema_version": schema, "job_id": JOB, "commit": COMMIT, "lane": 1,
            "nonce": NONCE, "request_sha256": "d" * 64, "result_sha256": writer.digest(result)}
