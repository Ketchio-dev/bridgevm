"""Minimal resident-agent client over a stream socket (QEMU virtserialport).

Wire (scripts/win-assets/bvagent.ps1): the guest says `READY <host> <ver>`, the
host sends `PING` (guest: `PONG`) and then `RUN <base64(utf8 cmd)>`, answered
by `OUT <exit> <b64>` or `OUTBEG <exit> <total> <n>`, `OUTCHUNK <seq> <b64>`
lines and `OUTEND <n>`.
"""

from __future__ import annotations

import base64
import socket
import time


class AgentWire:
    def __init__(self, path: str, deadline: float):
        self.buf = b""
        while True:
            try:
                self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                self.sock.connect(path)
                break
            except OSError:
                self.sock.close()
                if time.monotonic() > deadline:
                    raise TimeoutError(f"agent socket never accepted: {path}")
                time.sleep(0.2)

    def _line(self, deadline: float) -> str | None:
        while b"\n" not in self.buf:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            self.sock.settimeout(min(remaining, 3.0))
            try:
                data = self.sock.recv(65536)
            except socket.timeout:
                return ""
            if not data:
                raise ConnectionError("agent socket closed")
            self.buf += data
        line, self.buf = self.buf.split(b"\n", 1)
        return line.decode("utf-8", "replace").rstrip("\r")

    def send(self, line: str) -> None:
        self.sock.sendall(line.encode("utf-8") + b"\n")

    def wait_ready(self, deadline: float) -> str:
        """READY, or a PONG to a proactive PING when READY went out before we connected."""
        last_ping = 0.0
        while True:
            now = time.monotonic()
            if now - last_ping >= 3.0:
                self.send("PING")
                last_ping = now
            line = self._line(deadline)
            if line is None:
                raise TimeoutError("agent never became ready")
            if line.startswith("READY ") or line == "PONG":
                return line

    def run(self, command: str, deadline: float) -> tuple[int, str]:
        self.send("RUN " + base64.b64encode(command.encode("utf-8")).decode("ascii"))
        chunks: list[bytes] = []
        exit_code = None
        while True:
            line = self._line(deadline)
            if line is None:
                raise TimeoutError(f"no reply for: {command[:60]}")
            word, _, rest = line.partition(" ")
            if word == "OUT":
                code, _, b64 = rest.partition(" ")
                return int(code), base64.b64decode(b64).decode("utf-8", "replace")
            if word == "OUTBEG":
                exit_code = int(rest.split(" ")[0])
            elif word == "OUTCHUNK" and exit_code is not None:
                chunks.append(base64.b64decode(rest.partition(" ")[2]))
            elif word == "OUTEND" and exit_code is not None:
                return exit_code, b"".join(chunks).decode("utf-8", "replace")
