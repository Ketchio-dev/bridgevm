"""One fresh DEVELOPMENT_ONLY installation; no retries or T15 promotion."""
import hashlib
import os
import secrets
import signal
from pathlib import Path

from d11_fixture_commands import (stage_probe, prepare_commands, install_command, boot_command,
                                  stage_share, resource_identity, environment)
from d11_fixture_files import FileSeal, digest, identity, record
from d11_fixture_executables import admit
from d11_fixture_guest import FixtureController
from d11_fixture_mounts import Container
from d11_fixture_process import Processes, quiet_host
from t22_pair_admission import shutdown_observed


def interrupted(signum, frame): raise InterruptedError("fixture interrupted")


def seal_pair(work):
    hashes = {}
    for name, size, role in (("target.raw", 64 << 30, "disk_sha256"), ("vars.fd", 64 << 20, "vars_sha256")):
        path = work / name
        if path.lstat().st_size != size: raise ValueError("fixture output geometry differs")
        path.chmod(0o400)
        hashes[role] = digest(path)
    return hashes


def execute(directory, root, inputs, output, binary):
    value = {"schema": "bridgevm.d11-fixture-private.v1", "job_id": directory.name,
        "commit": inputs.rows["source_commit"][0], "input_manifest_sha256": hashlib.sha256(inputs.data).hexdigest(),
        "binary_sha256": inputs.rows["binary"][1], "installed": False, "ready_stopped": False,
        "sealed_fixture": False, "t15_ready": False, "cleanup_verified": False, "failure": "incomplete",
        "disk_sha256": "absent", "vars_sha256": "absent", "container_sha256": "absent",
        "guest_result_sha256": "absent", "source_sha256": "absent", "backing_identity": None,
        "processes": [], "output_identity": identity(output.lstat())[:2]}
    processes = Processes(output, directory, Path.home())
    probe_seal = None
    container = Container(output, inputs.path("iso"), processes, int(inputs.rows["container_gib"][0]))
    handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
    for sig in handlers: signal.signal(sig, interrupted)
    try:
        quiet_host()
        inputs.check()
        resource_identity(root, inputs)
        probe = stage_probe(root, binary, inputs.rows["binary"][1])
        probe_seal = FileSeal(probe, 128 << 20, inputs.rows["binary"][1])
        admit(inputs, probe, output, processes)
        container.create()
        work = container.mount
        (work / "tmp").mkdir(mode=0o700)
        nonce = secrets.token_hex(32)
        for index, (argv, env, timeout) in enumerate(prepare_commands(root, inputs, work, nonce)):
            inputs.check()
            container.verify()
            processes.run(argv, work / f"prepare-{index}.private.log", timeout, env)
        (work / "source.raw").chmod(0o400)
        value["source_sha256"] = digest(work / "source.raw")
        fd = os.open(work / "target.raw", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        try: os.ftruncate(fd, 64 << 30)
        finally: os.close(fd)
        inputs.check()
        probe_seal.check()
        processes.run(install_command(root, work), work / "install-wrapper.private.log", 1530, environment(work))
        # The install wrapper also verifies the last-written ESP marker.
        shutdown_observed(0, work / "install/run.log")
        probe_seal.check()
        value["installed"] = True
        helper = inputs.path("fixture_helper") / "d11-fixture-helper"
        processes.run([str(helper), "seed-vars", str(work / "vars.fd"), str(work / "target.raw")],
                      work / "seed.private.log", 60, environment(work))
        script_hash = stage_share(root, work)
        argv, env = boot_command(root, inputs, work, probe)
        inputs.check()
        container.verify()
        probe_seal.check()
        processes.launch(argv, work / "boot-wrapper.private.log", env)
        controller = FixtureController(work / "agent.ctl", work / "boot/run.log", work / "share",
                                       nonce, script_hash, processes)
        value["guest_result_sha256"] = controller.run()
        controller.write_command("shutdown.exe /s /t 0")
        status = processes.wait(60)
        shutdown_observed(status, work / "boot/run.log")
        processes.finish()
        probe_seal.check()
        value["ready_stopped"] = True
        inputs.check()
        container.verify()
        if digest(work / "source.raw") != value["source_sha256"]:
            raise ValueError("read-only installer changed")
        value.update(seal_pair(work))
        value["failure"] = "none"
    except BaseException as error:
        value["failure"] = "canceled" if isinstance(error, InterruptedError) else "incomplete"
        value["failure_type"] = type(error).__name__
    finally:
        for sig in handlers: signal.signal(sig, signal.SIG_IGN)
        try:
            processes.finish()
            container.cleanup()
            inputs.check()
            if probe_seal is not None: probe_seal.check()
            if identity(output.lstat())[:2] != value["output_identity"]:
                raise ValueError("fixture output replaced")
            value["cleanup_verified"] = processes.cleanup_complete
            if value["failure"] == "none" and value["ready_stopped"] and value["cleanup_verified"]:
                container.backing.chmod(0o400)
                value["container_sha256"] = digest(container.backing)
                value["backing_identity"] = identity(container.backing.lstat())
                value["sealed_fixture"] = True
        except BaseException as error:
            value.update(cleanup_verified=False, sealed_fixture=False, failure="cleanup-unproved")
            value["cleanup_failure_type"] = type(error).__name__
        finally:
            value["processes"] = processes.records
            try:
                record(output / "preparation.private.json", value)
            finally:
                if probe_seal is not None: probe_seal.close()
                for sig, handler in handlers.items(): signal.signal(sig, handler)
    return value
