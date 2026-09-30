"""Guest reset records as the HVF runtime prints them into run.log.

Product runs set BRIDGEVM_EXIT_ON_RESET=1 and print PROCESS_RECREATION; the
installed-boot wrapper reboots in process; a reached reboot limit stops the run.
tests/integration/hvf-reset-record-contract.py rebuilds these values from the
Rust format strings. Guest serial also reaches run.log, so only whole records match.
"""
import re

PROCESS_RECREATION = "stop: PSCI 0x84000009 exiting for process recreation (exit 42)"
IN_PROCESS_REBOOT_PREFIX = "PSCI SYSTEM_RESET: reboot "
REBOOT_LIMIT_PREFIX = "stop: PSCI 0x84000009 max reboot count "
REBOOT_LIMIT_SUFFIX = " reached"
AGENT_RESTARTS = ("BVAGENT READY", "BVAGENT re-READY", "BVAGENT SERVICE start")
_COUNTED = re.compile(re.escape(IN_PROCESS_REBOOT_PREFIX) + "[0-9]+/[0-9]+|"
                      + re.escape(REBOOT_LIMIT_PREFIX) + "[0-9]+" + re.escape(REBOOT_LIMIT_SUFFIX))


def guest_reset(line):
    line = line.removesuffix("\r")
    return line == PROCESS_RECREATION or _COUNTED.fullmatch(line) is not None


def require_same_session(lines, during):
    if any(line.startswith(AGENT_RESTARTS) or guest_reset(line) for line in lines):
        raise ValueError("guest restarted during " + during)
