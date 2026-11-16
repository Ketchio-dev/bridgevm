#!/usr/bin/env python3
"""Developer CLI for the installed app and its own-HVF guest agent (stdlib only)."""
from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time
import uuid

from vmctl_frame import capture


class ControlError(Exception):
    pass


def native(options, command, *operands, query=False):
    args = [options.bridgevm, "app", command, *operands]
    if options.library:
        args += ["--library", options.library]
    if query:
        args += ["--json"]
    result = subprocess.run(args, capture_output=query, text=True, check=False)
    if not query:
        return result.returncode
    if result.returncode:
        raise ControlError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout)


class Guest:
    def __init__(self, options):
        self.options = options
        self.identity = self.current()
        inventory = native(options, "inspect", options.vm, query=True)
        records = inventory["records"]
        if len(records) != 1 or records[0]["backendKind"] != "hvf-engine":
            raise ControlError("Select one saved own-HVF VM ID from list.")
        self.bundle = Path(records[0]["bundlePath"])
        self.evidence = self.bundle / "logs/hvf"
        self.check()

    def current(self):
        value = native(self.options, "status", self.options.vm, query=True)
        sessions = [s for s in value["sessions"] if s["ownership"] == "owned"]
        if len(sessions) != 1 or sessions[0].get("configurationMatch") != "same":
            raise ControlError("No unique app-owned run with matching saved configuration.")
        session = sessions[0]
        if self.options.command != "screenshot" and session.get("connectionState") != "connected":
            raise ControlError("Guest agent is not connected yet; wait and query status.")
        process = session["ownedProcess"]
        os.kill(process["processID"], 0)
        return (value["appInstanceID"], process["token"], process["processID"])

    def check(self):
        if self.current() != self.identity:
            raise ControlError("VM run changed; command was not retried.")

    def run(self, command, timeout=None):
        encoded = (command + "\n").encode()
        if len(encoded) > 87435:
            raise ControlError("Command exceeds guest control line limit.")
        log_path = self.evidence / "run.log"
        with regular(log_path, os.O_RDONLY) as log:
            # Only replies produced after this invocation may satisfy the request.
            log.seek(0, os.SEEK_END)
            self.check()
            with regular(self.bundle / "metadata/hvf.ctl", os.O_WRONLY | os.O_APPEND) as ctl:
                if os.write(ctl.fileno(), encoded) != len(encoded):
                    raise ControlError("Incomplete control write; command was not retried.")
            return self.reply(log, log_path, command, timeout)

    def reply(self, log, path, command, timeout=None):
        deadline = time.monotonic() + (self.options.timeout if timeout is None else timeout)
        result = b""
        if command.split(" ", 1)[0] in ("KEYINPUT", "TEXTINPUT", "POINTERINPUT"):
            command = " ".join(command.split(" ")[:2])  # relay redacts the input payload
        start = re.compile(rb"(?:^|\n)BVAGENT CMD " + re.escape(command.encode()) + rb" exit=(-?\d+)\r?\n")
        end = b"\nBVAGENT END " + command.encode()
        while time.monotonic() < deadline:
            os.kill(self.identity[2], 0)
            if os.fstat(log.fileno()).st_ino != path.stat().st_ino:
                raise ControlError("Run log replaced; command was not retried.")
            result += log.read(65536)
            match = start.search(result)
            if match:
                finish = result.find(end + b"\n", match.end())
                if finish < 0:
                    finish = result.find(end + b"\r\n", match.end())
                if finish >= 0:
                    self.check()
                    return int(match.group(1)), result[match.end():finish].decode("utf-8", "replace")
            if len(result) > 8 * 1024 * 1024:
                raise ControlError("Guest reply exceeded 8 MiB; command was not retried.")
            time.sleep(0.05)
        raise ControlError("Timed out waiting for guest reply; command may still run. It was not retried.")


def regular(path, flags):
    fd = os.open(path, flags | os.O_NOFOLLOW | os.O_NONBLOCK)
    info = os.fstat(fd)
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
        os.close(fd)
        raise ControlError("Expected an account-owned regular control/evidence file.")
    return os.fdopen(fd, "rb" if flags == os.O_RDONLY else "ab", buffering=0)


def powershell(script):
    # A unique prefix correlates this command even with another CLI or the GUI active.
    payload = ("# vmctl " + str(uuid.uuid4()) + "\n$ProgressPreference='SilentlyContinue'\n" + script).encode("utf-16-le")
    command = "powershell.exe -NoProfile -NonInteractive -EncodedCommand " + base64.b64encode(payload).decode()
    if len(command) > 8157:  # cmd.exe /c wrapper leaves this much of Windows' 8191-character limit
        raise ControlError("PowerShell source exceeds the guest cmd.exe limit; place a larger script in the shared folder and invoke its guest path.")
    return command


def input_command(verb, value):
    return verb + " " + str(uuid.uuid4()) + " " + base64.b64encode(value.encode()).decode()


def send_text(guest, text):
    if not text or len(text.encode("utf-16-le")) > 131072:
        raise ControlError("Text must contain 1..65536 UTF-16 code units.")
    deadline = time.monotonic() + guest.options.timeout
    # A single SendInput array lost intermediate VK_PACKET characters in live WinUI Notepad.
    # Receipt-paced codepoints preserve Korean and supplementary characters without the clipboard.
    for index, char in enumerate(text):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ControlError(f"Text deadline exceeded after {index} codepoints; no input was replayed.")
        code, output = guest.run(input_command("TEXTINPUT", char), timeout=remaining)
        if code:
            return code, f"Text stopped after {index} codepoints.\n" + output
    return 0, f"BVCLI_TEXT inserted_codepoints={len(text)} (guest input receipts)"


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--bridgevm", default="/Applications/BridgeVM.app/Contents/Resources/target/release/bridgevm")
    result.add_argument("--library", help="Native app library (absolute path)")
    result.add_argument("--timeout", type=float, default=40, help="Guest reply deadline, seconds; no automatic retry")
    commands = result.add_subparsers(dest="command", required=True)
    for name in ("list", "inspect", "readiness", "status", "start", "stop"):
        command = commands.add_parser(name, help="Delegate to bridgevm app " + name)
        if name != "list":
            command.add_argument("vm", help="Exact VM ID")
        command.add_argument("--json", action="store_true")
    for name in ("exec", "key", "text", "pointer", "resize", "screenshot"):
        command = commands.add_parser(name)
        command.add_argument("vm", help="Exact running own-HVF VM ID")
        if name == "exec":
            command.add_argument("--file", type=Path, help="PowerShell source; otherwise read stdin")
        elif name == "screenshot":
            command.add_argument("output", type=Path, help="New PNG path (never overwritten)")
        else:
            command.add_argument("value", help={"key": "e.g. enter, ctrl+v, win+r", "text": "Unicode text (no clipboard use)", "pointer": "e.g. click:16000x16000 or rightclick:16000x16000; coordinates 0..32767", "resize": "WidthxHeight, e.g. 2560x1600"}[name])
    return result


def main(argv=None):
    options = parser().parse_args(argv)
    try:
        if not 0 < options.timeout <= 900:
            raise ControlError("--timeout must be between 0 and 900 seconds.")
        if options.command in ("list", "inspect", "readiness", "status", "start", "stop"):
            operands = ([] if options.command == "list" else [options.vm]) + (["--json"] if options.json else [])
            return native(options, options.command, *operands)
        guest = Guest(options)
        if options.command == "text":
            code, output = send_text(guest, options.value)
            print(output)
            return code if 0 <= code <= 255 else 1
        if options.command == "screenshot":
            with regular(guest.evidence / "display.fb", os.O_RDONLY) as source:
                png, metadata = capture(source, min(options.timeout, 5))
            guest.check()
            with options.output.open("xb") as output:
                output.write(png)
            print(json.dumps(dict(metadata, output=str(options.output))))
            return 0
        if options.command == "exec":
            script = options.file.read_text(encoding="utf-8-sig") if options.file else sys.stdin.read()
            if not script.strip():
                raise ControlError("Supply PowerShell on stdin or with --file.")
            command = powershell(script)
        elif options.command == "resize":
            match = re.fullmatch(r"([1-9][0-9]{2,4})x([1-9][0-9]{2,4})", options.value)
            if not match or any(int(n) > 16384 for n in match.groups()):
                raise ControlError("Expected WIDTHxHEIGHT, each 100..16384.")
            command = "DISPLAYMODE " + " ".join(match.groups())
        else:
            verb = {"key": "KEYINPUT", "pointer": "POINTERINPUT"}[options.command]
            command = input_command(verb, options.value)
        code, output = guest.run(command)
        print(output)
        if options.command == "resize" and not re.search(r"\bresult=0(?:\s|$)", output):
            return 1
        return code if 0 <= code <= 255 else 1
    except (ControlError, OSError, ValueError, KeyError) as error:
        print("vmctl: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
