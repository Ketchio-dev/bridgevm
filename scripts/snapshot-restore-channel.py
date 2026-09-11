#!/usr/bin/env python3
"""Fresh, exact, bounded guest-command receipts for the restore-boot gate."""
from pathlib import Path
import sys
import time


class Receipt:
    def __init__(self, command):
        self.header = ("BVAGENT CMD " + command + " exit=0").encode()
        self.end = ("BVAGENT END " + command).encode()
        self.pending = bytearray()
        self.output = bytearray()
        self.started = False

    def feed(self, data):
        self.pending.extend(data)
        while b"\n" in self.pending:
            raw, _, remainder = self.pending.partition(b"\n")
            self.pending = bytearray(remainder)
            line = raw.rstrip(b"\r")
            if len(raw) > 262144:
                raise ValueError("oversized agent record")
            if not self.started:
                if line == self.header:
                    self.started = True
                continue
            if line == self.end:
                return bytes(self.output)
            if line.startswith((b"BVAGENT CMD ", b"BVAGENT END ", b"BVAGENT SERVICE ")):
                raise ValueError("interrupted command receipt")
            self.output.extend(line + b"\n")
            if len(self.output) > 65536:
                raise ValueError("oversized command output")
        if len(self.pending) > 262144:
            raise ValueError("oversized partial agent record")
        return None


def exchange(ctl, log, command, timeout):
    if not command or "\n" in command or "\r" in command or timeout <= 0:
        raise ValueError("invalid command or deadline")
    receipt = Receipt(command)
    deadline = time.monotonic() + timeout
    with open(log, "rb") as stream:
        stream.seek(0, 2)
        if stream.tell():
            stream.seek(-1, 2)
            if stream.read(1) != b"\n":
                raise ValueError("incomplete preexisting agent record")
        with open(ctl, "a", encoding="utf-8") as control:
            control.write(command + "\n")
        while time.monotonic() < deadline:
            if Path(log).stat().st_size < stream.tell():
                raise ValueError("agent log truncated")
            chunk = stream.read(65536)
            if chunk:
                result = receipt.feed(chunk)
                if result is not None:
                    return result
            else:
                time.sleep(0.05)
    raise TimeoutError("no fresh matching successful command and END")


if __name__ == "__main__":
    try:
        control, log_path, command_text, seconds, output_path = sys.argv[1:]
        result = exchange(control, log_path, command_text, float(seconds))
        Path(output_path).write_bytes(result)
    except (OSError, ValueError, TimeoutError) as error:
        print("FAIL: snapshot agent receipt: " + str(error), file=sys.stderr)
        sys.exit(1)
