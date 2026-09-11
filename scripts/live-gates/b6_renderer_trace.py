"""Fixed, sealed renderer-trace policy and bounded host-log evidence."""
import hashlib
import json
import os
import pathlib

from b6_cell_inputs import regular_file
TIER = "d3-b6-renderer-trace"
POLICY = {"VREND_DEBUG": "shader,cmd,obj,d3d", "max_log_bytes": 128 * 1024 * 1024,
          "purpose": "renderer diagnosis only", "claim_eligible": False}
POLICY_BYTES = (json.dumps(POLICY, indent=2, sort_keys=True) + "\n").encode()
POLICY_HASH = hashlib.sha256(POLICY_BYTES).hexdigest()
CONFOUNDERS = ["Fixed renderer logging may perturb execution",
               "VREND_DEBUG=shader,cmd,obj,d3d is set by the sealed diagnostic tier",
               "Diagnostic library builds may enable additional assertions",
               "Trace collection is not performance or product evidence"]

def log_evidence(path, limit=POLICY["max_log_bytes"]):
    digest = hashlib.sha256()
    tgsi = glsl = count = 0
    with regular_file(path) as stream:
        before = os.fstat(stream.fileno())
        if before.st_size > limit:
            raise ValueError("renderer log exceeds the declared size limit")
        for raw in stream:
            count += len(raw)
            if count > limit:
                raise ValueError("renderer log grew beyond its limit")
            digest.update(raw)
            line = raw.strip()
            if line.startswith((b"proxy: failed to exec ", b"failed to initialize venus renderer")):
                raise ValueError("renderer startup failed; captured frames cannot validate this observation")
            tgsi += line.removeprefix(b"venus-win32: TGSI received:venus-win32: ") in (b"FRAG", b"VERT", b"GEOM", b"TESS_CTRL", b"TESS_EVAL", b"COMP")
            glsl += line.removeprefix(b"venus-win32: GLSL:venus-win32: ").startswith(b"#version ")
        after = os.fstat(stream.fileno())
    identity = lambda item: (item.st_dev, item.st_ino, item.st_size, item.st_mtime_ns)
    if identity(before) != identity(after) or identity(after) != identity(os.stat(path, follow_symlinks=False)):
        raise ValueError("renderer log changed while it was authenticated")
    return {"sha256": digest.hexdigest(), "bytes": count, "tgsi_headers": tgsi, "glsl_headers": glsl}


def finish_trace(core, value, out):
    logs = {stage: log_evidence(pathlib.Path(out) / stage / "run.log") for stage in ("scale", "capture")}
    present = logs["capture"]["tgsi_headers"] > 0 and logs["capture"]["glsl_headers"] > 0
    summary = {"schema_version": 1, "valid": present, "environment_policy_sha256": POLICY_HASH,
               "logs": logs, "pass": False, "claim_eligible": False,
               "criterion_pass": False, "capability_promotion": False}
    policy_path = pathlib.Path(out) / "renderer-trace-policy.json"
    summary_path = pathlib.Path(out) / "renderer-trace-summary.json"
    core.write_json(policy_path, POLICY)
    if core.file_hash(policy_path) != POLICY_HASH:
        raise ValueError("trace policy file differs from its sealed definition")
    core.write_json(summary_path, summary)
    value["raw_sha256"] = core.file_hash(summary_path)
    value["evidence_paths"] += ["renderer-trace-policy.json", "renderer-trace-summary.json"]
    value.update({"pass": False, "claim_eligible": False, "criterion_pass": False, "capability_promotion": False})
    if not present:
        value["run_count"] = 0
        raise ValueError("capture log lacks TGSI or GLSL trace evidence")
