"""Deliver loader values after the trusted test shell has already started."""
import subprocess


def run_in_trusted_shell(command, env):
    initial = dict(env)
    values = [initial.pop(name, "") for name in ("DYLD_INSERT_LIBRARIES", "LD_PRELOAD")]
    body = 'export DYLD_INSERT_LIBRARIES="$1" LD_PRELOAD="$2"; shift 2; ' + command[0]
    return subprocess.run(["/bin/bash", "--noprofile", "--norc", "-p", "-c", body,
                           command[1], *values, *command[2:]], env=initial,
                          capture_output=True, text=True, timeout=35)
