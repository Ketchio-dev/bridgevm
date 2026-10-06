"""Fixed packaged no-vTPM boot command for owned development clones."""
from pathlib import Path
import subprocess

from guest_input_live_inputs import digest
from t22_pair_controller import GUEST_SHARE
from t22_pair_environment import controlled_env


def boot_command(rows, clones, work):
    app = Path(rows["app_bundle"][0])
    resources = app / "Contents/Resources"
    wrapper = resources / "scripts/run-hvf-windows-installed-boot.sh"
    policy = resources / "scripts/run-hvf-windows-scripted-install-policy.sh"
    firmware = resources / "firmware/edk2-aarch64-secure-code.fd"
    for asset in (wrapper, policy, firmware):
        if asset.is_symlink() or not asset.is_file() or asset.resolve() != asset:
            raise ValueError("fixed packaged boot resource is unsafe")
    env = controlled_env()
    selected = subprocess.check_output(["/bin/bash", "--noprofile", "--norc", "-p", "-c",
        'source "$1"; select_scripted_install_firmware "$2" || exit; printf "%s" "$FIRMWARE_SHA256"',
        "t22-firmware", str(policy), str(resources)], text=True, timeout=30, env=env)
    if selected != digest(firmware):
        raise ValueError("packaged firmware does not match its pinned policy")
    # Use the packaged wrapper: its release policy rejects PATH/repository
    # binary overrides and selects the authenticated app's own executable.
    command = ["/bin/bash", "--noprofile", "--norc", "-p", str(wrapper), "--target", str(clones["image"]),
               "--vars", str(clones["vars"]), "--firmware-code", str(firmware),
               "--evidence-dir", str(work / "boot"), "--release", "--skip-build",
               "--watchdog-ms", "450000", "--max-reboots", "0", "--ram-mib", "4096",
               "--smp-cpus", "4", "--max-exits", "50000000", "--no-guest-disk-harvest",
               "--agent-service-control", str(work / "agent.ctl"), "--agent-share-host", str(work / "share"),
               "--agent-share-guest", GUEST_SHARE, "--agent-share-ms", "500", "--agent-share-max-kb", "8192"]
    return command, env, firmware
