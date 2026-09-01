#!/usr/bin/env python3
"""Host-authenticate nonce-bound T17 guest observations and product logs."""
from __future__ import annotations
import hashlib, json, re
from pathlib import Path

OBSERVATIONS = ("keyboard_pointer_challenge_sha256", "clipboard_roundtrip_sha256", "share_host_to_guest_sha256", "share_guest_to_host_sha256", "network_result_sha256", "audio_result_sha256", "audio_playback_count", "audio_error_count", "snapshot_marker_a_sha256", "snapshot_marker_b_sha256", "snapshot_marker_restored_a_sha256")
LOG_FIELDS = ("first_run_log_sha256", "mutation_run_log_sha256", "final_run_log_sha256", "agent_result_sha256", "first_ready_offset", "first_ready_line_nonce_sha256", "first_shutdown_offset", "first_shutdown_line_nonce_sha256", "mutation_ready_offset", "mutation_ready_line_nonce_sha256", "mutation_shutdown_offset", "mutation_shutdown_line_nonce_sha256", "final_ready_offset", "final_ready_line_nonce_sha256", "second_shutdown_offset", "second_shutdown_line_nonce_sha256")

def _unique(pairs: list[tuple[str, object]]) -> dict:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate guest evidence field: {key}")
        result[key] = value
    return result

def _bytes(path: Path, root: Path, limit: int) -> bytes:
    try:
        path.relative_to(root)
    except ValueError as error:
        raise ValueError("guest observation escapes its product root") from error
    cursor = path.parent
    while cursor != root:
        if cursor.is_symlink():
            raise ValueError("guest observation has a symlink parent")
        cursor = cursor.parent
    if not path.is_file() or path.is_symlink() or not 0 < path.stat().st_size <= limit:
        raise ValueError(f"guest observation is missing, unsafe, or oversized: {path.name}")
    return path.read_bytes()

def _json(path: Path, root: Path) -> dict:
    value = json.loads(_bytes(path, root, 1024 * 1024).decode("utf-8"), object_pairs_hook=_unique)
    if not isinstance(value, dict):
        raise ValueError(f"guest evidence is not an object: {path.name}")
    return value

def _digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def _line(data: bytes, offset: object) -> str:
    if type(offset) is not int or offset < 0 or offset >= len(data) or (offset and data[offset - 1] != 0x0a):
        raise ValueError("guest log offset is invalid")
    end = data.find(b"\n", offset)
    if end < 0 or end - offset > 1024 * 1024:
        raise ValueError("guest log line is unterminated or oversized")
    raw = data[offset:end]
    if raw.endswith(b"\r"):
        raw = raw[:-1]
    return raw.decode("utf-8")

def _audio(data: bytes) -> None:
    lines = [line for line in data.decode("utf-8").splitlines() if line.startswith("hda CoreAudio stats:")]
    if not lines:
        raise ValueError("first product log has no CoreAudio counters")
    values: dict[str, int] = {}
    for key in ("frames_rendered", "drops", "callback_errors"):
        matches = re.findall(rf"(?:^|\s){key}=(\d+)(?=\s|$)", lines[-1])
        if len(matches) != 1:
            raise ValueError(f"CoreAudio counter {key} is missing or ambiguous")
        values[key] = int(matches[0])
    if values["frames_rendered"] <= 0 or values["drops"] != 0 or values["callback_errors"] != 0:
        raise ValueError("CoreAudio counters do not prove clean playback")

def verify(request: dict) -> list[Path]:
    nonce, prefix = request["nonce"], request["nonce"][:12]
    bundle = Path(request["disk_path"]).parent.parent
    share = Path(request["share_path"])
    evidence_root = bundle / "metadata/product-e2e"
    evidence_path = Path(request["guest_evidence_path"])
    evidence = _json(evidence_path, bundle)
    identity = {"schema_version": "bridgevm.windows-product-e2e-guest-evidence.v2", "job_id": request["job_id"], "commit": request["commit"], "lane": request["lane"], "nonce": nonce, "vm_slug": request["vm_slug"]}
    if set(evidence) != set(identity) | set(OBSERVATIONS) | set(LOG_FIELDS) or any(evidence.get(key) != value for key, value in identity.items()):
        raise ValueError("guest evidence identity or exact fields are invalid")
    clipboard = f"브리지VM T17 클립보드 왕복 v1\n{nonce}\n".encode()
    raw_specs = {
        "keyboard_pointer_challenge_sha256": (share / f"t17-keyboard-pointer-{prefix}.txt", f"bridgevm-t17-keyboard-pointer-v1\n{nonce}\n".encode()),
        "clipboard_roundtrip_sha256": (share / f"t17-clipboard-guest-{prefix}.txt", clipboard),
        "share_host_to_guest_sha256": (share / f"t17-{prefix}.txt", f"bridgevm-t17-share-v1\n{nonce}\n".encode()),
        "share_guest_to_host_sha256": (share / f"t17-guest-{prefix}.txt", f"bridgevm-t17-guest-share-v1\n{nonce}\n".encode()),
        "network_result_sha256": (share / f"t17-network-{prefix}.txt", f"bridgevm-t17-network-ok-v1\n{nonce}\n".encode()),
        "audio_result_sha256": (share / f"t17-audio-{prefix}.txt", f"bridgevm-t17-audio-ok-v1\n{nonce}\n".encode()),
        "snapshot_marker_a_sha256": (share / f"t17-snapshot-a-{prefix}.txt", f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode()),
        "snapshot_marker_b_sha256": (share / f"t17-snapshot-b-{prefix}.txt", f"bridgevm-t17-snapshot-b-v1\n{nonce}\n".encode()),
        "snapshot_marker_restored_a_sha256": (share / f"t17-snapshot-restored-a-{prefix}.txt", f"bridgevm-t17-snapshot-a-v1\n{nonce}\n".encode()),
    }
    host_clipboard = share / f"t17-clipboard-host-{prefix}.txt"
    if _bytes(host_clipboard, share, 8192) != clipboard:
        raise ValueError("host clipboard challenge bytes are invalid")
    for key, (path, expected) in raw_specs.items():
        actual = _bytes(path, share, 8192)
        if actual != expected or evidence.get(key) != _digest(actual):
            raise ValueError(f"raw guest observation is invalid: {path.name}")
    if type(evidence.get("audio_playback_count")) is not int or evidence["audio_playback_count"] < 1 or evidence.get("audio_error_count") != 0:
        raise ValueError("guest audio counters are invalid")
    agent_path = evidence_root / "agent-result.json"
    guest_agent_path = share / f"t17-agent-result-{prefix}.json"
    agent_data = _bytes(agent_path, bundle, 1024 * 1024)
    if agent_data != _bytes(guest_agent_path, share, 1024 * 1024) or evidence.get("agent_result_sha256") != _digest(agent_data):
        raise ValueError("managed and shared guest agent results differ")
    agent = _json(agent_path, bundle)
    agent_identity = {"schema_version": "bridgevm.windows-product-e2e-agent-result.v2", **{key: value for key, value in identity.items() if key != "schema_version"}}
    if set(agent) != set(agent_identity) | set(OBSERVATIONS) or any(agent.get(key) != value for key, value in agent_identity.items()) or any(agent.get(key) != evidence.get(key) for key in OBSERVATIONS):
        raise ValueError("guest agent result is unbound or disagrees with raw evidence")
    logs = {"first": evidence_root / "first-run.log", "mutation": evidence_root / "mutation-run.log", "final": bundle / "logs/hvf/run.log"}
    log_data = {name: _bytes(path, bundle, 64 * 1024 * 1024) for name, path in logs.items()}
    for name in logs:
        if evidence.get(f"{name}_run_log_sha256") != _digest(log_data[name]):
            raise ValueError(f"{name} product log hash is invalid")
    line_specs = (("first", "first-ready", "first_ready_offset", "first_ready_line_nonce_sha256", "BVAGENT READY"), ("first", "first-shutdown", "first_shutdown_offset", "first_shutdown_line_nonce_sha256", "stop: PSCI SYSTEM_OFF"), ("mutation", "mutation-ready", "mutation_ready_offset", "mutation_ready_line_nonce_sha256", "BVAGENT READY"), ("mutation", "mutation-shutdown", "mutation_shutdown_offset", "mutation_shutdown_line_nonce_sha256", "stop: PSCI SYSTEM_OFF"), ("final", "final-ready", "final_ready_offset", "final_ready_line_nonce_sha256", "BVAGENT READY"), ("final", "second-shutdown", "second_shutdown_offset", "second_shutdown_line_nonce_sha256", "stop: PSCI SYSTEM_OFF"))
    offsets: dict[str, list[int]] = {name: [] for name in logs}
    for name, event, offset_field, hash_field, marker in line_specs:
        line = _line(log_data[name], evidence.get(offset_field)); offsets[name].append(evidence[offset_field])
        bound = _digest(f"bridgevm-t17-{event}-v1\n{nonce}\n{line}\n".encode())
        if not line.startswith(marker) or evidence.get(hash_field) != bound:
            raise ValueError(f"guest log observation {offset_field} is invalid")
    if any(not values[0] < values[1] for values in offsets.values()):
        raise ValueError("guest READY/SYSTEM_OFF order is invalid")
    _audio(log_data["first"])
    return [evidence_path, *logs.values(), agent_path, guest_agent_path, host_clipboard, *[path for key, (path, _) in raw_specs.items() if key != "share_host_to_guest_sha256"]]
