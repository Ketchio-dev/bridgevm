"""Synthetic same-user owner for ordinary CLI transport tests; never launches AppKit."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import threading
import uuid


class StatusFixture:
    def __init__(self, library):
        self.library = library.resolve()
        info = self.library.stat()
        identity = f"bridgevm-native-runtime-v1\n{os.geteuid()}\n{info.st_dev}\n{info.st_ino}\n"
        digest = hashlib.sha256(identity.encode()).hexdigest()[:32]
        parent = Path(f"/private/tmp/bridgevm-app-{os.geteuid()}")
        parent.mkdir(mode=0o700, exist_ok=True)
        assert parent.is_dir() and not parent.is_symlink()
        assert parent.stat().st_uid == os.geteuid() and parent.stat().st_mode & 0o777 == 0o700
        self.directory = parent / digest
        self.directory.mkdir(mode=0o700)
        self.lock = os.open(self.directory / "owner.lock", os.O_CREAT | os.O_RDWR, 0o600)
        fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(str(self.directory / "status.sock"))
        os.chmod(self.directory / "status.sock", 0o600)
        self.server.listen(4)
        self.server.settimeout(0.1)
        self.requests = []
        self.errors = []
        self.instance = str(uuid.uuid4()).upper()
        self.stopped = threading.Event()
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    @staticmethod
    def read(peer, count):
        chunks = bytearray()
        while len(chunks) < count:
            chunk = peer.recv(count - len(chunks))
            assert chunk, "CLI closed before completing request"
            chunks.extend(chunk)
        return bytes(chunks)

    def serve(self):
        while not self.stopped.is_set():
            try:
                peer, _ = self.server.accept()
            except socket.timeout:
                continue
            try:
                with peer:
                    peer.settimeout(3)
                    size = struct.unpack(">I", self.read(peer, 4))[0]
                    assert 0 < size <= 8192
                    request = json.loads(self.read(peer, size))
                    assert request["operation"] == "status"
                    self.requests.append(request)
                    digest = request["savedConfiguration"].get("digest")
                    session = {"ownership": "owned", "connectionState": "booting",
                               "configurationMatch": "same" if digest else "unknown",
                               "ownedProcess": {"token": self.instance, "processID": os.getpid()}}
                    if digest:
                        session["acceptedConfigurationDigest"] = digest
                    response = {"schema": "bridgevm.app-runtime.v1", "requestID": request["requestID"],
                                "library": request["library"], "vmID": request["vmID"],
                                "appInstanceID": self.instance, "observedAt": 1789500000,
                                "scope": "app-observation", "sessions": [session]}
                    payload = json.dumps(response, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
                    peer.sendall(struct.pack(">I", len(payload)) + payload)
            except Exception as error:
                self.errors.append(repr(error))

    def close(self):
        self.stopped.set()
        self.thread.join(timeout=4)
        assert not self.thread.is_alive(), "Fixture did not stop"
        self.server.close()
        os.close(self.lock)
        shutil.rmtree(self.directory)
        assert not self.errors, self.errors
