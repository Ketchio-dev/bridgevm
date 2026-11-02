#!/usr/bin/env python3
"""D4 mount cancellation escapes subprocess wait OSError fallbacks; no real hdiutil runs."""
import json
import os
from pathlib import Path
import plistlib
import signal
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import winpe_companion_mounts as mounts
from signal_driver_support import swallowing_wait, terminate_when_ready

FAKE = """#!{}
import os, pathlib, plistlib, sys, time
if sys.argv[1] == "info": sys.stdout.buffer.write(plistlib.dumps({{"images": []}})); raise SystemExit(0)
if sys.argv[1] != "attach": raise SystemExit(0)
pathlib.Path({!r}).write_text(str(os.getpid()))
time.sleep(30)
"""
DRIVER = """import json, pathlib, sys
sys.path.insert(0, sys.argv[1])
from winpe_companion_mounts import MountSafetyError, mounted_image
base = pathlib.Path(sys.argv[2])
try:
    with mounted_image(base / "image.raw", base / "mount"): outcome = {"yielded": True}
except MountSafetyError as error:
    outcome = {"cleanup_complete": error.cleanup_complete, "cause": type(error.__cause__).__name__}
(base / "result.json").write_text(json.dumps(outcome))
"""


class MountSignal(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name).resolve()
        self.image = self.parent / "image.raw"
        self.image.write_bytes(b"private-fixture")
        self.root = self.parent / "mount-before"
        entities = [{"dev-entry": "/dev/disk77"},
                    {"dev-entry": "/dev/disk77s1", "mount-point": str(self.root / "Windows")}]
        owned = {"image-path": str(self.image), "system-entities": entities}
        self.inventories = [[], [owned], [owned], []]
        self.attach = SimpleNamespace(returncode=0, stdout=plistlib.dumps({"system-entities": entities}))

    def command(self, argv, **kwargs):
        if argv[1] == "info":
            return SimpleNamespace(returncode=0, stdout=plistlib.dumps({"images": self.inventories.pop(0)}))
        if argv[1] == "attach":
            swallowing_wait()
            return self.attach
        return SimpleNamespace(returncode=0, stdout=b"")

    def test_attach_wait_cannot_swallow_cancellation(self):
        previous, body = signal.getsignal(signal.SIGTERM), []
        with self.assertRaises(mounts.MountSafetyError) as caught:
            with mock.patch.object(mounts.subprocess, "run", side_effect=self.command), \
                    mounts.mounted_image(self.image, self.root):
                body.append("inspected")
        self.assertEqual(body, [])
        self.assertFalse(caught.exception.cleanup_complete)
        self.assertEqual(type(caught.exception.__cause__).__name__, "MountOperationCancelled")
        self.assertTrue(self.root.is_dir())
        self.assertIs(signal.getsignal(signal.SIGTERM), previous)

    def test_real_term_stops_a_piped_attach(self):
        tools, ready = self.parent / "bin", self.parent / "attach.pid"
        tools.mkdir(); fake = tools / "hdiutil"
        fake.write_text(FAKE.format(sys.executable, str(ready))); fake.chmod(0o700)
        # A fake that cannot exec would let PATH search fall through to the host tool.
        probe = subprocess.run([str(fake), "info", "-plist"], capture_output=True, check=True, timeout=10)
        self.assertEqual(plistlib.loads(probe.stdout), {"images": []})
        (self.parent / "driver.py").write_text(DRIVER)
        env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ.get("PATH", ""))
        status = terminate_when_ready([self.parent / "driver.py", ROOT / "scripts/live-gates", self.parent],
                                      ready, env=env)
        self.assertEqual(status, 0, "TERM did not end the attach within five seconds")
        self.assertEqual(json.loads((self.parent / "result.json").read_text()),
                         {"cleanup_complete": False, "cause": "MountOperationCancelled"})
        self.assertTrue((self.parent / "mount").is_dir())
        with self.assertRaises(ProcessLookupError): os.kill(int(ready.read_text()), 0)


if __name__ == "__main__":
    unittest.main()
