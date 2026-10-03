"""Real, tiny host-only qcow2 fixtures for the generic bundle import gate."""
from __future__ import annotations

import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
NAME = "portable-source"
DISK_SNAPSHOTS = ("legacy snapshot", "base", "base-create")


def run(*args: str | Path, timeout: float = 60) -> str:
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, text=True,
                            capture_output=True, timeout=timeout)
    if result.returncode:
        raise AssertionError(f"command failed ({result.returncode}): {args!r}\n"
                             f"{result.stdout}\n{result.stderr}")
    return result.stdout


def binaries() -> tuple[Path, Path]:
    toolchain = os.environ.get("BRIDGEVM_CHECK_TOOLCHAIN", "+1.97.0")
    run("cargo", toolchain, "build", "--locked", "-p", "bridgevm-cli",
        "-p", "bridgevm-daemon", timeout=600)
    metadata = json.loads(run("cargo", toolchain, "metadata", "--no-deps",
                              "--format-version", "1", "--locked"))
    target = Path(metadata["target_directory"]) / "debug"
    return target / "bridgevm", target / "bridgevmd"


def required_qemu() -> tuple[str, str]:
    paths = tuple(shutil.which(name) for name in ("qemu-img", "qemu-io"))
    if not all(paths):
        raise AssertionError("real qemu-img and qemu-io are required; no fixture skip")
    versions = [run(path, "--version").splitlines()[0] for path in paths]
    majors = [int(re.search(r"version (\d+)\.", version).group(1)) for version in versions]
    expected = os.environ.get("BRIDGEVM_PORTABILITY_QEMU_MAJOR")
    if majors[0] != majors[1] or majors[0] < 8 or (expected and majors[0] != int(expected)):
        raise AssertionError(f"required matching QEMU major {expected or '>=8'}: {versions}")
    print("; ".join(versions), flush=True)
    return paths


def read_json(path: Path) -> dict | list:
    return json.loads(path.read_text())


def export_metadata(path: Path) -> dict:
    if path.is_dir():
        return read_json(path / "metadata/export.json")
    with tarfile.open(path) as archive:
        with archive.extractfile("metadata/export.json") as receipt:
            return json.load(receipt)


def digests(bundle: Path) -> dict[str, str]:
    return {str(path.relative_to(bundle)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(bundle.rglob("*")) if path.is_file()
            and path.suffix not in (".sock", ".lock")}


def payload_digest(path: Path):
    return digests(path) if path.is_dir() else hashlib.sha256(path.read_bytes()).hexdigest()


def refuse(*args: str | Path) -> str:
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, text=True,
                            capture_output=True, timeout=60)
    if result.returncode == 0:
        raise AssertionError(f"unsafe fixture import unexpectedly succeeded: {args!r}")
    return result.stdout + result.stderr


def endpoint(cli: Path, *, store: Path | None = None,
             sock: Path | None = None):
    prefix = ("--store", store) if store is not None else ("--socket", sock)
    return lambda *args: run(cli, *prefix, *args)


def request(sock: Path, **payload) -> dict:
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(30)
        stream.connect(str(sock))
        stream.sendall(json.dumps(payload).encode() + b"\n")
        with stream.makefile("rb") as response:
            raw = response.readline(1024 * 1024)
    if not raw.endswith(b"\n"):
        raise AssertionError("missing bounded complete daemon response")
    value = json.loads(raw)
    if value.get("type") == "error":
        raise AssertionError(f"daemon rejected request: {value}")
    return value


@contextlib.contextmanager
def daemon(binary: Path, store: Path):
    store.mkdir(parents=True)
    sock = store / "run/bridgevmd.sock"
    with (store / "daemon.log").open("wb") as log:
        child = subprocess.Popen([str(binary), "--store", str(store)],
                                 cwd=ROOT, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 10
            while not sock.exists():
                if child.poll() is not None or time.monotonic() >= deadline:
                    detail = (store / "daemon.log").read_text(errors="replace")[-8192:]
                    raise AssertionError(f"owned fixture daemon failed to become ready: {detail}")
                time.sleep(0.025)
            yield sock
        finally:
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            else:
                child.wait(timeout=5)


def io(qemu_io: str, path: Path, operation: str, pattern: int, offset: int) -> None:
    run(qemu_io, "-f", "qcow2", "-c",
        f"{operation} -P {pattern} {offset} 4096", path)


def source_fixture(root: Path, cli: Path, qemu_img: str, qemu_io: str) -> tuple[Path, dict]:
    source = root / "source store"
    call = endpoint(cli, store=source)
    call("create", NAME, "--os", "ubuntu", "--arch", "x86_64",
         "--mode", "compatibility", "--disk", "8M", "--disk-format", "qcow2")
    call("disk", "create", NAME)
    bundle = source / f"vms/{NAME}.vmbridge"
    active = bundle / "disks/root.qcow2"
    io(qemu_io, active, "write", 17, 0)
    for index, name in enumerate(DISK_SNAPSHOTS, 1):
        call("snapshot", "create", NAME, name, "--kind", "disk")
        slug = name.replace(" ", "-")
        metadata = read_json(bundle / f"metadata/snapshot-disks/{slug}.json")
        overlay = Path(metadata["overlay_path"])
        # QEMU 8 and 11 both accept the explicit legacy backing-format flag.
        run(qemu_img, "create", "-f", "qcow2", "-b", active,
            "-F", "qcow2", overlay)
        call("snapshot", "disk-create", NAME, name)
        io(qemu_io, overlay, "write", 17 * (index + 1), 4096 * index)
        active = overlay
    receipt = bundle / "metadata/snapshot-disks/creates/legacy-snapshot.json"
    receipt.rename(bundle / "metadata/snapshot-disks/legacy-snapshot-create.json")
    call("snapshot", "create", NAME, "sleep point", "--kind", "suspend")
    suspend = read_json(bundle / "metadata/suspend-images/sleep-point.json")
    Path(suspend["image_path"]).write_bytes(b"synthetic suspend fixture; no guest state")
    call("port", "add", NAME, "2222:22")
    share = root / "approved workspace"
    share.mkdir()
    call("share", "add", NAME, "workspace", share,
         "--host-path-token", "fixture-share-token")
    metadata = bundle / "metadata"
    (metadata / "machine-identifier.bin").write_bytes(b"synthetic-machine-identity")
    (metadata / "network-mac-address.txt").write_text("02:00:00:00:00:71\n")
    (metadata / "retained-evidence.json").write_text('{"fixture":"preserve me"}\n')
    (metadata / "qmp.sock").write_text("excluded fixture placeholder\n")
    (metadata / "export.lock").write_text("excluded fixture placeholder\n")
    call("snapshot", "restore", NAME, "base-create")
    call("snapshot", "disk-create", NAME, "base-create")
    return bundle, digests(bundle)
