"""Fixed native installer/boot commands and sealed helper staging."""
import os
from pathlib import Path
import shutil

from d11_fixture_files import digest, write
from d11_fixture_guest import SCRIPT, SHARE
from d11_fixture_inputs import RESOURCES, SEED, POLICY
from t22_pair_environment import controlled_env


def stage_probe(root, binary, expected):
    destination = root / "target/release/examples/hvf_gic_boot_probe"
    parent = root
    for component in ("target", "release", "examples"):
        parent = parent / component
        if os.path.lexists(parent):
            if parent.is_symlink() or not parent.is_dir() or parent.stat().st_uid != os.geteuid():
                raise ValueError("fixture probe parent is not owned")
        else:
            parent.mkdir(mode=0o700)
    if destination.resolve() != destination or os.path.lexists(destination):
        raise ValueError("fixture worktree probe path is not exclusive")
    with binary.open("rb") as source, destination.open("xb") as output:
        shutil.copyfileobj(source, output)
    destination.chmod(0o500)
    if digest(destination) != expected: raise ValueError("fixture staged binary differs")
    return destination


def environment(work):
    env = controlled_env()
    env["TMPDIR"] = str(work / "tmp")
    return env


def prepare_commands(root, inputs, work, nonce):
    helper = inputs.path("fixture_helper") / "d11-fixture-helper"
    env = environment(work)
    env.update({"ISO": str(inputs.path("iso")), "OUT": str(work / "source.raw"),
        "SIZE_BYTES": str(16 << 30), "SWM_SPLIT_MB": "3800",
        "ASSETS": str(root / "scripts/win-assets"),
        "WIMLIB": str(inputs.path("tools") / "wimlib-imagex"),
        "WINDOWS_FILE_COMPARE": str(inputs.path("tools") / "bv-file-compare.exe"),
        "WINDOWS_GUEST_PAYLOAD_DIR": str(inputs.path("payload")),
        "WINDOWS_GUEST_PAYLOAD_MANIFEST": str(inputs.path("payload_manifest")),
        "WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER": str(inputs.path("tools") / "bridgevm-catalog-verify"),
        "WINDOWS_UNATTEND_PATH": str(work / "unattend.private.xml")})
    return [
        ([str(helper), "unattend", str(work / "unattend.private.xml"), nonce], environment(work), 30),
        ([str(helper), "initial-vars", str(work / "install-vars.fd"), "DEVELOPMENT_ONLY"], environment(work), 60),
        (["/bin/bash", "--noprofile", "--norc", "-p", str(root / "scripts/build-hvf-windows-scripted-source.sh")], env, 1200)]


def install_command(root, work):
    return ["/bin/bash", "--noprofile", "--norc", "-p", str(root / "scripts/run-hvf-windows-scripted-install.sh"),
            "--source", str(work / "source.raw"), "--target", str(work / "target.raw"),
            "--vars", str(work / "install-vars.fd"), "--evidence-dir", str(work / "install"),
            "--release", "--skip-build", "--watchdog-ms", "1500000", "--max-reboots", "8", "--ram-mib", "4096"]


def boot_command(root, inputs, work, binary):
    env = environment(work)
    env["BRIDGEVM_PREBUILT_PROBE"] = str(binary)
    argv = ["/bin/bash", "--noprofile", "--norc", "-p", str(root / "scripts/run-hvf-windows-installed-boot.sh"),
            "--target", str(work / "target.raw"), "--vars", str(work / "vars.fd"),
            "--firmware-code", str(inputs.path("firmware")), "--evidence-dir", str(work / "boot"),
            "--release", "--skip-build", "--watchdog-ms", "450000", "--max-reboots", "8", "--ram-mib", "4096",
            "--smp-cpus", "4", "--max-exits", "50000000", "--no-guest-disk-harvest",
            "--agent-service-control", str(work / "agent.ctl"), "--agent-share-host", str(work / "share"),
            "--agent-share-guest", SHARE, "--agent-share-ms", "500", "--agent-share-max-kb", "8192"]
    return argv, env


def stage_share(root, work):
    (work / "share").mkdir(mode=0o700)
    write(work / "agent.ctl", b"")
    source = root / "scripts/win-assets" / SCRIPT
    write(work / "share" / SCRIPT, source.read_bytes())
    value = digest(source)
    if digest(work / "share" / SCRIPT) != value: raise ValueError("fixture script copy differs")
    return value


def resource_identity(root, inputs):
    source = root / "apps/macos/Sources/BridgeVMControl/Resources"
    helper = inputs.path("fixture_helper") / RESOURCES
    if any(digest(source / name) != digest(helper / name) for name in (SEED, POLICY)):
        raise ValueError("fixture resources differ from sealed source")
    firmware = root / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
    if digest(firmware) != inputs.rows["firmware"][1]: raise ValueError("installer firmware differs")
