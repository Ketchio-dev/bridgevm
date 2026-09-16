#!/usr/bin/env python3
"""Private fake child for the actual app/runner channel integration test."""
import json
import os
from pathlib import Path
import select
import signal
import socket
import sys
import time


def main(config):
    root, role = Path(config["root"]), config["role"]
    stopped = False
    listeners = []
    key_count = 0

    def stop(_signal, _frame):
        nonlocal stopped
        stopped = True

    signal.signal(signal.SIGTERM, stop)
    if role == "swtpm":
        key = sys.stdin.buffer.read(32)
        assert len(key) == 32 and key == bytes([0x42]) * 32
        key_count = len(key)
        del key
        paths = [arg.split("path=", 1)[1].split(",", 1)[0]
                 for arg in sys.argv if "path=" in arg]
        assert len(paths) == 2
        for path in paths:
            listener = socket.socket(socket.AF_UNIX)
            listener.bind(path)
            listener.listen(2)
            listener.setblocking(False)
            listeners.append(listener)
    else:
        assert os.environ["BRIDGEVM_VIRTIO_CONSOLE_CTL"] == config["control"]
    identity = {"role": role, "pid": os.getpid(), "parent": os.getppid(),
                "nonce": config["nonce"], "keyBytesRead": key_count}
    temporary = root / (role + ".started.tmp")
    temporary.write_text(json.dumps(identity))
    temporary.replace(root / (role + ".started"))
    if role == "helper":
        print("BVAGENT SERVICE start t=1", flush=True)
        print("BVAGENT READY host=fixture t=2", flush=True)
    deadline = time.monotonic() + 15
    try:
        while not stopped and time.monotonic() < deadline:
            if (root / "fixture-release").exists():
                break
            if role == "helper":
                commands = Path(config["control"]).read_text()
                if "shutdown.exe /p /f\n" in commands:
                    (root / "guest-command-observed").write_text("owned fixture only\n")
                    break
            ready, _, _ = select.select(listeners, [], [], 0.01)
            for listener in ready:
                stream, _ = listener.accept()
                with stream:
                    stream.settimeout(0.1)
                    if stream.recv(4) == b"\0\0\0\1":
                        stream.sendall(b"\0\0\0\0\0\0\0\1")
    finally:
        for listener in listeners:
            listener.close()


if __name__ == "__main__":
    main(json.loads(__FIXTURE_CONFIG_LITERAL__))
