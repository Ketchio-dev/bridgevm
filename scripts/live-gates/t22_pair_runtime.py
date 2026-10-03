"""One bounded no-vTPM boot of exclusive development-only clones."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time

from guest_input_live_inputs import clone_media, digest
from guest_input_owned_group import stop
from retained_windows_identity import directory_identity
from t22_pair_admission import screen_disk, shutdown_observed
from t22_pair_controller import PairController, SCRIPT, GUEST_SHARE
from t22_pair_provenance import unchanged, file_hash
from t22_pair_environment import controlled_env


def interrupt(signum, frame):
    raise InterruptedError("owned pair preparation interrupted")


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


def execute(rows, manifest, data, documents, output, receipt):
    work = output / "live"
    paths = {key: Path(rows[key][0]) for key in ("image", "vars", "binary")}
    hashes = {key: rows[key][1] for key in paths}
    process, attempted, clones, work_identity = None, False, {}, None
    clone_identities = {}
    receipt.update({"cleanup_complete": False, "complete": False, "source_integrity": False})
    previous = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    for sig in previous:
        signal.signal(sig, interrupt)
    try:
        receipt.update(screen_disk(paths["image"]))
        unchanged(manifest, data, documents, rows)
        clones = clone_media(paths, hashes, work)
        work_identity = directory_identity(work)
        clone_identities = {k: (p.stat().st_dev, p.stat().st_ino) for k, p in clones.items()}
        share = work / "share"; share.mkdir(mode=0o700)
        control = work / "agent.ctl"; control.touch(mode=0o600, exist_ok=False)
        root = Path(__file__).resolve().parents[2]
        source, target = root / "scripts/win-assets" / SCRIPT, share / SCRIPT
        script_hash = digest(source)
        with source.open("rb") as incoming, target.open("xb") as outgoing:
            shutil.copyfileobj(incoming, outgoing)
        if digest(target) != script_hash:
            raise ValueError("pinned query copy differs")
        command, env, firmware = boot_command(rows, clones, work)
        receipt.update({"query_script_sha256": script_hash, "firmware_sha256": digest(firmware),
                        "boot_config_sha256": __import__("hashlib").sha256("\0".join(command).encode()).hexdigest(),
                        "vtpm_configured": False, "initial_boot_writes_owned_clones": True})
        subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", rows["app_bundle"][0]],
                       check=True, timeout=30, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        unchanged(manifest, data, documents, rows)
        if directory_identity(work) != work_identity:
            raise ValueError("owned clone directory changed before launch")
        with (work / "launcher.log").open("xb") as log:
            attempted = True
            process = subprocess.Popen(command, cwd=root, env=env, stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
            driver = PairController(control, work / "boot/run.log", share, script_hash)
            deadline = time.monotonic() + 300
            while not any(line.startswith("BVAGENT SERVICE start") for line in driver.lines()):
                if process.poll() is not None or time.monotonic() >= deadline:
                    raise TimeoutError("already provisioned guest agent unavailable")
                time.sleep(.25)
            driver.deadline = time.monotonic() + 120
            facts, receipt["query_result_sha256"] = driver.run()
            receipt.update(facts)
            receipt["encryption_observed_after_initial_boot"] = True
            driver.write_command("shutdown.exe /s /t 0")
            status = process.wait(timeout=60)
            receipt["shutdown_log_sha256"] = shutdown_observed(status, work / "boot/run.log")
            receipt["natural_shutdown_observed"] = True
    except BaseException as error:
        receipt["failure_type"] = type(error).__name__
    finally:
        for sig in previous:
            signal.signal(sig, signal.SIG_IGN)
        try:
            # Child teardown never depends on an untrusted pathname probe.
            receipt["cleanup_complete"] = False if attempted and process is None else stop(process)
            owned = work_identity is not None and directory_identity(work) == work_identity
            if not owned:
                receipt.update({"cleanup_complete": False, "ownership_uncertain": True})
            if owned and receipt["cleanup_complete"]:
                unchanged(manifest, data, documents, rows)
                for name, path in clones.items():
                    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
                    try:
                        info = os.fstat(fd)
                        if (info.st_dev, info.st_ino) != clone_identities[name] or info.st_nlink != 1:
                            raise ValueError("owned clone identity changed")
                        os.fchmod(fd, 0o400)
                    finally:
                        os.close(fd)
                receipt["output_hashes"] = {k: file_hash(p, readonly=True) for k, p in clones.items()}
                receipt.update({"complete": True, "source_integrity": True})
        except BaseException as error:
            receipt.update({"cleanup_complete": False, "complete": False,
                            "source_integrity": False, "cleanup_failure_type": type(error).__name__})
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
    if receipt["cleanup_complete"]:
        try:
            unchanged(manifest, data, documents, rows)
            if digest(source) != script_hash:
                raise ValueError("query source changed during preparation")
        except (OSError, ValueError, UnboundLocalError) as error:
            receipt.update({"complete": False, "source_integrity": False, "failure_type": type(error).__name__})
    ready = (not receipt.get("failure_type") and receipt.get("complete") is True
             and receipt.get("cleanup_complete") is True and receipt.get("source_integrity") is True
             and receipt.get("natural_shutdown_observed") is True
             and receipt.get("encryption_observed_after_initial_boot") is True)
    receipt["preparation_complete"] = ready
    receipt["complete"] = ready
    return clones if ready else None
