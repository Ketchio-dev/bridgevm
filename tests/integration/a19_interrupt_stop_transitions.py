"""Real owned read-FD transitions must refuse after the stop-time recheck."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import a19_interrupt_restore_child as observer
import a19_interrupt_stop_points as points
from a19_interrupt_stop_fixtures import HOLD_READ, fixture, helper

ROOT = Path(__file__).resolve().parents[2]


def lsof_interlock(root: Path, stage: Path, managed: Path, destination: Path, point) -> Path:
    path = root / "lsof-interlock.py"
    path.write_text("#!/usr/bin/env python3\nimport json,os,pathlib,subprocess,sys,time\n" +
        f"sys.path.insert(0,{str(ROOT / 'scripts/live-gates')!r})\n" +
        "import a19_interrupt_stop_points as points\n" +
        f"root=pathlib.Path({str(root)!r});stage=pathlib.Path({str(stage)!r})\n" +
        f"managed=pathlib.Path({str(managed)!r});dest=pathlib.Path({str(destination)!r})\n" +
        f"point=points.STOP_POINTS[{point.name!r}]\n" +
        "if not (root/'entered.json').exists():\n"
        "    data=json.dumps({'ready':points.staged_ready(point,stage),\n"
        "                     'selection':points.selection(point,managed,dest)}).encode()\n"
        "    fd=os.open(root/'entered.json',os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)\n"
        "    os.write(fd,data);os.fsync(fd);os.close(fd)\n"
        "    (root/'advance').touch()\n"
        "until=time.monotonic()+2\n"
        "while not (root/'helper.py.held').exists():\n"
        "    if time.monotonic()>until: raise SystemExit(11)\n"
        "    time.sleep(.005)\n" +
        f"result=subprocess.run([{observer.LSOF!r},*sys.argv[1:]],capture_output=True,timeout=2)\n" +
        "name='fd-after-stop' if (root/'fd-before-stop').exists() else 'fd-before-stop'\n"
        "fd=os.open(root/name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)\n"
        "os.write(fd,result.stdout);os.fsync(fd);os.close(fd)\n"
        "sys.stdout.buffer.write(result.stdout);sys.stderr.buffer.write(result.stderr)\n"
        "raise SystemExit(result.returncode)\n")
    path.chmod(0o700)
    return path


class StopTransitionContract(unittest.TestCase):
    def setUp(self):
        if not Path(observer.LSOF).is_file():
            if sys.platform == "darwin":
                self.fail("macOS T22 test needs owner-readable lsof")
            self.skipTest("lsof is only required on the physical Mac and hosted macOS")

    def transition(self, point, mutation, *, selection_changes, readiness_changes):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(strict=True)
            disk, vars, snapshot, output, managed = fixture(root, point == points.SWAP_RESTORE)
            destination = root / "export" if point == points.CREATE_EXPORT else snapshot
            stage = points.staging(point, managed, destination)
            before = points.selection(point, managed, destination)
            source_pair = [p.read_bytes() for p in (disk, vars)]
            hold = (f"root=pathlib.Path({str(root)!r})\nuntil=time.monotonic()+5\n"
                    "while not (root/'advance').exists():\n"
                    "    if time.monotonic()>until: raise SystemExit(10)\n"
                    "    time.sleep(.005)\n" + mutation + HOLD_READ)
            names = ("disk.raw", "vars.fd") if point == points.CREATE_EXPORT else (
                "disk.raw", "vars.fd", "manifest.json")
            path = helper(root / "helper.py", f"stage=pathlib.Path({str(stage)!r})\n", names, hold=hold)
            interlock = lsof_interlock(root, stage, managed, destination, point)
            children = []
            original_popen = subprocess.Popen

            def spawn(argv, **kwargs):
                child = original_popen(argv, **kwargs)
                if argv[0] == str(path):
                    children.append(child)
                return child

            try:
                with patch.object(observer, "LSOF", str(interlock)), patch.object(subprocess, "Popen", side_effect=spawn):
                    with self.assertRaisesRegex(RuntimeError, "^staged read or old selection changed at stop$"):
                        observer.run(path, destination, disk, vars, output, 3, point.name)
                self.assertEqual(len(children), 1)
                self.assertEqual(children[0].returncode, -9)
                with self.assertRaises(ChildProcessError):
                    os.waitpid(children[0].pid, os.WNOHANG)
            finally:
                # The fixture owns these exact handles even if an assertion fails.
                for child in children:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)
            entered = json.loads((root / "entered.json").read_bytes())
            self.assertEqual(entered, {"ready": True, "selection": before})
            self.assertTrue(Path(f"{path}.held").is_file())
            for name in ("fd-before-stop", "fd-after-stop"):
                self.assertTrue(observer.read_fd_observed((root / name).read_text(), children[0].pid, stage / "disk.raw"))
            self.assertEqual(points.staged_ready(point, stage), not readiness_changes)
            self.assertEqual(points.selection(point, managed, destination) != before, selection_changes)
            self.assertEqual([p.read_bytes() for p in (disk, vars)], source_pair)
            for name in ("interrupt-helper-fd.private.log", "interrupt-helper-context.private.json"):
                self.assertFalse((output / name).exists())

    def test_create_selection_changes_during_observation_refuse(self):
        self.transition(points.CREATE_EXPORT,
            "dest=pathlib.Path(sys.argv[4]);dest.mkdir();(dest/'manifest.json').write_bytes(b'new')\n",
            selection_changes=True, readiness_changes=False)

    def test_create_manifest_appears_during_observation_refuses(self):
        self.transition(points.CREATE_EXPORT, "(stage/'manifest.json').write_bytes(b'late-manifest')\n",
            selection_changes=False, readiness_changes=True)

    def test_swap_selection_changes_during_observation_refuse(self):
        self.transition(points.SWAP_RESTORE,
            "current=stage.parent/'current';(current/'next').write_bytes(b'new')\n"
            "os.replace(current/'next',current/'disk.raw')\n",
            selection_changes=True, readiness_changes=False)
