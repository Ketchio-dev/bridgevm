"""Pure classification of a B9 diagnostic failure at its observed stage."""

from b9_asset_error import invalid_evidence_failure

def classify_failure(stage: str, error: Exception, receipt: dict) -> str:
    if invalid_evidence_failure(stage, error):
        return "INVALID_EVIDENCE"
    if stage == "vlc":
        if receipt.get("ready_sha256") and not receipt.get("collector_sha256"):
            return "COLLECTOR_FAILED"
        return "PLAYBACK_INCOMPLETE" if receipt.get("collector_sha256") else "GUEST_NOT_READY"
    if stage == "playback" or (stage == "shutdown" and
                               receipt["result_class"] == "VLC_PID_PRESENTS_CAPTURED"):
        return "PLAYBACK_INCOMPLETE"
    return receipt["result_class"]
