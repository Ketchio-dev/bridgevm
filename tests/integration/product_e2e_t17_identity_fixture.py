"""T17 request fixture with a real identity-bound allocation."""
import json
import product_e2e_identity_fixtures as fixture
from product_e2e_work_fixture import allocate, WORK

def prepare_t17(case):
    writer = fixture.T17
    prefix = fixture.NONCE[:12]
    value = {"schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2",
             "job_id": fixture.JOB, "commit": fixture.COMMIT, "campaign_mode": "pilot",
             "lane": 1, "nonce": fixture.NONCE, "three_d_injection": False,
             "vm_name": f"BridgeVM T17 Lane 1 {prefix}", "vm_slug": f"bridgevm-t17-lane-1-{prefix}",
             **{field: f"/private/tmp/fixture/{field}" for field in writer.REQUEST_PATHS}}
    root = allocate(case.root.parent, kind="e2e")
    value.update(WORK.capture(root, fixture.JOB, "e2e", 1)); value["lane_root"] = str(root)
    case.request.write_text(json.dumps(value))
    case.result.write_text(json.dumps(fixture.lane(writer)))
