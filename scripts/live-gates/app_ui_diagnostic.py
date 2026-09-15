#!/usr/bin/env python3
"""Run one sealed app-only native window diagnostic, never a guest workload."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import signal
import stat
import struct
import subprocess
import sys
import time
import zlib

TIER = "d6-app-ui"
FORMAT = "bridgevm-app-ui-v1"
TEST_CLASS = "BridgeVMControlTests.HvfAppUIRenderTests"
DEADLINE_SECONDS = 90
SCREENSHOTS = (
    "welcome-light-default", "welcome-light-minimum", "welcome-dark-default",
    "welcome-dark-minimum", "create-sheet", "import-form", "overview", "overview-search",
)
ACTIONS = (
    "welcome_visible", "create_opened", "create_cancelled", "import_opened",
    "overview_opened", "search_filtered", "search_cleared",
)
TRIPWIRES = ("model_creations", "runtime_creations", "install_creations", "file_jobs")


def regular(path, maximum):
    path = Path(path)
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or not 0 < info.st_size <= maximum:
        raise ValueError("expected bounded regular file")
    return path


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def parse_manifest(path, commit):
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("invalid commit")
    rows = regular(path, 16_384).read_text(encoding="utf-8").splitlines()
    fields = {}
    for line in rows:
        values = line.split("\t")
        if not values or values[0] in fields:
            raise ValueError("duplicate manifest field")
        fields[values[0]] = values[1:]
    if set(fields) != {"format", "commit", "binary"}:
        raise ValueError("manifest fields differ")
    if fields["format"] != [FORMAT] or fields["commit"] != [commit]:
        raise ValueError("manifest format or commit differs")
    binary = fields["binary"]
    if len(binary) != 2 or not Path(binary[0]).is_absolute():
        raise ValueError("binary must have an absolute path and digest")
    if not re.fullmatch(r"[0-9a-f]{64}", binary[1]):
        raise ValueError("invalid binary digest")
    return Path(binary[0]), binary[1]


def verify_binary(path, expected):
    path = regular(path, 1024 * 1024 * 1024)
    with path.open("rb") as source:
        header = source.read(32)
    if len(header) != 32 or header[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("expected a 64-bit Mach-O test bundle executable")
    cpu, _, kind = struct.unpack_from("<III", header, 4)
    if cpu not in (0x0100000C, 0x01000007) or kind != 8:
        raise ValueError("expected a native Mach-O bundle")
    if digest(path) != expected:
        raise ValueError("sealed binary digest differs")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def png_layout(header):
    width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", header)
    depths = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
    if (not 0 < width <= 8192 or not 0 < height <= 8192 or color not in depths
            or depth not in depths[color] or compression != 0 or filtering != 0 or interlace not in (0, 1)):
        raise ValueError("unsupported PNG header")
    bits = depth * {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[color]
    passes = ((0, 0, 1, 1),) if interlace == 0 else (
        (0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4),
        (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2))
    rows = []
    for x, y, dx, dy in passes:
        columns = max(0, (width - x + dx - 1) // dx)
        count = max(0, (height - y + dy - 1) // dy)
        if columns and count:
            rows.append(((columns * bits + 7) // 8 + 1, count))
    total = sum(length * count for length, count in rows)
    if total > 128 * 1024 * 1024:
        raise ValueError("decoded PNG exceeds bounds")
    return (width, height), color, depth, rows, total


def verify_png_stream(compressed, rows, expected):
    decoder = zlib.decompressobj()
    try:
        decoded = decoder.decompress(compressed, expected + 1)
    except zlib.error as error:
        raise ValueError("PNG image data cannot be decoded") from error
    if (len(decoded) != expected or not decoder.eof or decoder.unused_data
            or decoder.unconsumed_tail):
        raise ValueError("PNG image data does not match its scanlines")
    offset = 0
    for length, count in rows:
        for _ in range(count):
            if decoded[offset] > 4:
                raise ValueError("invalid PNG row filter")
            offset += length


def png_dimensions(path):
    data = regular(path, 32 * 1024 * 1024).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("screenshot is not PNG")
    offset, layout, palette = 8, None, False
    compressed, data_ended = [], False
    while offset + 12 <= len(data):
        length = struct.unpack_from(">I", data, offset)[0]
        end = offset + 12 + length
        if end > len(data):
            raise ValueError("truncated PNG chunk")
        kind = data[offset + 4:offset + 8]
        payload = data[offset + 8:end - 4]
        if not re.fullmatch(b"[A-Za-z]{4}", kind):
            raise ValueError("invalid PNG chunk type")
        if zlib.crc32(kind + payload) != struct.unpack_from(">I", data, end - 4)[0]:
            raise ValueError("PNG chunk checksum differs")
        if offset == 8:
            if kind != b"IHDR" or length != 13:
                raise ValueError("PNG lacks initial dimensions")
            layout = png_layout(payload)
        elif kind == b"IHDR":
            raise ValueError("duplicate PNG dimensions")
        if kind == b"PLTE":
            if (palette or compressed or layout[1] in (0, 4) or length % 3
                    or not 0 < length <= 768 or (layout[1] == 3 and length // 3 > 2 ** layout[2])):
                raise ValueError("invalid PNG palette")
            palette = True
        if kind == b"IDAT":
            if data_ended or (layout[1] == 3 and not palette):
                raise ValueError("invalid PNG image data ordering")
            compressed.append(payload)
        elif compressed:
            data_ended = True
        if kind not in (b"IHDR", b"PLTE", b"IDAT", b"IEND") and not kind[0] & 32:
            raise ValueError("unsupported critical PNG chunk")
        if kind == b"IEND":
            if length or end != len(data) or not compressed:
                raise ValueError("PNG ending or image data differs")
            verify_png_stream(b"".join(compressed), layout[3], layout[4])
            return layout[0]
        offset = end
    raise ValueError("PNG is incomplete")


def verify_observations(output):
    report = regular(output / "ui-observations.json", 65_536)
    value = json.loads(report.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    expected = {"schema_version", "kind", "fixture_data", "guest_behavior_proof",
                "screenshots", "actions", "tripwires", "failure"}
    if not isinstance(value, dict) or set(value) != expected:
        raise ValueError("observation schema differs")
    if type(value["schema_version"]) is not int or value["schema_version"] != 1:
        raise ValueError("observation version differs")
    if (value["kind"] != "native-app-ui-diagnostic" or value["fixture_data"] is not True
            or value["guest_behavior_proof"] is not False or value["failure"] is not None):
        raise ValueError("observation is not a completed app-only fixture")
    actions, tripwires = value["actions"], value["tripwires"]
    if not isinstance(actions, dict) or set(actions) != set(ACTIONS):
        raise ValueError("action schema differs")
    if not all(actions[key] is True for key in ACTIONS):
        raise ValueError("a native UI action was not observed")
    if not isinstance(tripwires, dict) or set(tripwires) != set(TRIPWIRES):
        raise ValueError("tripwire schema differs")
    if not all(type(tripwires[key]) is int and tripwires[key] == 0 for key in TRIPWIRES):
        raise ValueError("domain work was attempted")
    captures = value["screenshots"]
    if not isinstance(captures, list) or len(captures) != len(SCREENSHOTS):
        raise ValueError("screenshot set is incomplete")
    seen = set()
    for item in captures:
        if not isinstance(item, dict) or set(item) != {"name", "file", "sha256", "width", "height"}:
            raise ValueError("screenshot schema differs")
        name = item["name"]
        if not isinstance(name, str) or name not in SCREENSHOTS or name in seen:
            raise ValueError("screenshot name differs")
        seen.add(name)
        if item["file"] != name + ".png" or not isinstance(item["sha256"], str):
            raise ValueError("screenshot filename differs")
        path = output / item["file"]
        size = png_dimensions(path)
        if any(type(item[key]) is not int for key in ("width", "height")):
            raise ValueError("screenshot dimensions are not integers")
        if size != (item["width"], item["height"]) or digest(path) != item["sha256"]:
            raise ValueError("screenshot dimensions or digest differ")
    return digest(report)


def load_job(output, commit, manifest, binary_digest):
    fields = {}
    for line in regular(output / "job.env", 16_384).read_text().splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in fields:
            raise ValueError("invalid job record")
        fields[key] = value
    if (fields.get("tier") != TIER or fields.get("commit") != commit
            or fields.get("input_manifest_sha256") != digest(manifest)
            or fields.get("sealed_binary_sha256") != binary_digest
            or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", fields.get("job_id", ""))):
        raise ValueError("job seals differ")
    return fields["job_id"]


def run_child(bundle, output, canceled):
    environment = {key: os.environ[key] for key in ("HOME", "USER", "LOGNAME") if key in os.environ}
    environment.update(PATH="/usr/bin:/bin:/usr/sbin:/sbin", BRIDGEVM_APP_UI_DIAGNOSTIC="1",
                       BRIDGEVM_APP_UI_OUTPUT=str(output))
    command = ["/usr/bin/xcrun", "xctest", "-XCTest", TEST_CLASS, str(bundle)]
    with (output / "native-ui.log").open("xb") as log:
        child = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + DEADLINE_SECONDS
        try:
            while child.poll() is None:
                if canceled() or time.monotonic() >= deadline:
                    raise TimeoutError("native UI canceled or exceeded deadline")
                time.sleep(0.1)
            if canceled():
                raise TimeoutError("native UI canceled")
            return child.returncode
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=2)


def run(output, repo, commit, manifest, binary):
    output, repo, manifest, binary = map(Path, (output, repo, manifest, binary))
    receipt = {"tier": TIER, "commit": commit, "claim_eligible": False, "criterion_pass": False,
               "capability_promotion": False, "boots_attempted": 0, "run_count": 0,
               "sample_count": 0, "passes": 0, "failures": 0, "pass": False,
               "outcome": "diagnostic-incomplete", "failure_code": "preflight-refused"}
    interrupted = False

    def stop(_signum, _frame):
        nonlocal interrupted
        interrupted = True

    previous = {kind: signal.signal(kind, stop) for kind in (signal.SIGTERM, signal.SIGINT)}
    try:
        if not output.is_dir() or output.is_symlink():
            raise ValueError("job directory is unavailable")
        _, expected = parse_manifest(manifest, commit)
        receipt["input_manifest_sha256"] = digest(manifest)
        receipt["binary_hash"] = expected
        receipt["job_id"] = load_job(output, commit, manifest, expected)
        verify_binary(binary, expected)
        actual_commit = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
        if actual_commit != commit or sys.platform != "darwin":
            raise ValueError("requires the sealed revision on macOS")
        canceled = lambda: interrupted or (output / "cancel.requested").exists()
        if canceled():
            raise TimeoutError("canceled before native UI launch")
        private = output / "app-ui-private"
        private.mkdir(mode=0o700)
        bundle = private / "BridgeVMAppPackageTests.xctest"
        executable = bundle / "Contents/MacOS/BridgeVMAppPackageTests"
        executable.parent.mkdir(parents=True)
        shutil.copyfile(binary, executable)
        executable.chmod(0o500)
        verify_binary(executable, expected)
        with (bundle / "Contents/Info.plist").open("xb") as info:
            plistlib.dump({"CFBundleExecutable": "BridgeVMAppPackageTests", "CFBundlePackageType": "BNDL",
                          "CFBundleIdentifier": "dev.bridgevm.app-ui-diagnostic"}, info)
        receipt["run_count"] = 1
        receipt["failure_code"] = "native-ui-failed"
        if run_child(bundle, private, canceled) != 0:
            raise ValueError("native UI test process failed")
        receipt["failure_code"] = "observation-refused"
        receipt["result_sha256"] = verify_observations(private)
        if canceled():
            raise TimeoutError("canceled before accepting UI observations")
        receipt.update(outcome="diagnostic-complete", failure_code="none", sample_count=1, passes=1, **{"pass": True})
    except TimeoutError as error:
        receipt.update(outcome="diagnostic-incomplete", failure_code="canceled-or-deadline")
        print(str(error), file=sys.stderr)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"app UI diagnostic refused: {error}", file=sys.stderr)
    finally:
        for kind, handler in previous.items():
            signal.signal(kind, handler)
        if receipt["run_count"] and not receipt["pass"]:
            receipt["failures"] = 1
        with (output / "receipt.json").open("x", encoding="utf-8") as result:
            json.dump(receipt, result, indent=2, sort_keys=True)
            result.write("\n")
    return 0 if receipt["pass"] else 1


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "validate-manifest":
        binary, expected = parse_manifest(sys.argv[2], sys.argv[3])
        verify_binary(binary, expected)
        return 0
    if len(sys.argv) == 7 and sys.argv[1] == "run":
        return run(*sys.argv[2:])
    raise ValueError("usage: app_ui_diagnostic.py validate-manifest MANIFEST COMMIT | run OUT REPO COMMIT MANIFEST BINARY")


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f"app UI diagnostic refused: {error}", file=sys.stderr)
        sys.exit(2)
