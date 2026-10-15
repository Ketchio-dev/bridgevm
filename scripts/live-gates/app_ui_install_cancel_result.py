"""A complete scenario requires independent exits, AX transcript and captures."""
from app_ui_host_cleanup import finite_number
from app_ui_host_v2_bundle import verify_bundles
from app_ui_host_v2_cleanup import read_json
from app_ui_install_cancel_json import protocol_record
from app_ui_install_cancel_cleanup import verify_exit
from app_ui_install_cancel_observations import verify_observations
from app_ui_install_cancel_protocol import verify_driver


def verify_result(private, fields, launcher_exit):
    launch = verify_exit(private, fields)
    if launcher_exit != 0 or launch["success"] is not True:
        raise ValueError("install cancellation supervisor did not complete")
    driver_hash, reads = verify_driver(private, launch)
    output = private / "host-observations"
    report_hash = verify_observations(output, reads)
    identity = read_json(output / "host-identity.json", ("schema_version", "kind", "pid",
        "bundle_identifier", "bundle_path", "executable_path", "executable_sha256", "started_uptime"))
    owner = launch["host"]
    if (type(identity["schema_version"]) is not int or identity["schema_version"] != 1
            or identity["kind"] != "native-app-ui-host-identity" or type(identity["pid"]) is not int
            or any(identity[key] != owner[key] for key in ("pid", "bundle_identifier", "bundle_path", "executable_path", "executable_sha256"))
            or not finite_number(identity["started_uptime"], 0)):
        raise ValueError("install cancellation host identity differs")
    completion = protocol_record(output / "install-cancel-host-completion.json",
        "native-app-ui-install-cancel-host-completion",
        {"pid", "success", "settled", "fixturePreserved", "reportSHA256"}, {"failureCode"})
    if (type(completion["pid"]) is not int or completion["pid"] != owner["pid"]
            or any(completion[key] is not True for key in ("success", "settled", "fixturePreserved"))
            or completion["reportSHA256"] != report_hash or completion.get("failureCode") is not None):
        raise ValueError("install cancellation host completion differs")
    verify_bundles(private, fields)
    return report_hash, driver_hash
