#!/usr/bin/env python3
"""Socket-only swtpm stand-in for launcher lifecycle smoke tests."""

import os
import hashlib
import pathlib
import signal
import socket
import sys
import time


def option_value(name: str) -> str:
    try:
        return sys.argv[sys.argv.index(name) + 1]
    except (ValueError, IndexError) as exc:
        raise SystemExit(f"missing {name}") from exc


def comma_value(option: str, key: str) -> str:
    for field in option.split(","):
        if field.startswith(f"{key}="):
            return field.split("=", 1)[1]
    raise SystemExit(f"missing {key} in {option}")


if len(sys.argv) < 2 or sys.argv[1] != "socket":
    raise SystemExit("expected socket subcommand")

data_path = comma_value(option_value("--server"), "path")
control_path = comma_value(option_value("--ctrl"), "path")
state_dir = pathlib.Path(comma_value(option_value("--tpmstate"), "dir"))
state_dir.mkdir(parents=True, exist_ok=True)
(state_dir / "fake-state.persistent").write_text("preserve me\n", encoding="utf-8")

if "--key" in sys.argv:
    key_option = option_value("--key")
    if key_option != "fd=0,format=binary,mode=aes-256-cbc":
        raise SystemExit(f"unexpected --key value: {key_option}")
    key = b""
    while len(key) < 32:
        chunk = os.read(0, 32 - len(key))
        if not chunk:
            break
        key += chunk
    if len(key) != 32 or os.read(0, 1):
        raise SystemExit("expected exactly 32 binary key bytes on stdin")
    (state_dir / "fake-key.sha256").write_text(
        hashlib.sha256(key).hexdigest() + "\n", encoding="ascii"
    )

listeners = []
for path in (data_path, control_path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    listener.bind(path)
    listener.listen(1)
    listeners.append(listener)

running = True


def stop(_signum: int, _frame: object) -> None:
    global running
    running = False


signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
while running:
    time.sleep(0.05)

for listener in listeners:
    listener.close()
