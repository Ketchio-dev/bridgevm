#!/usr/bin/env python3
"""T17 lane key forget failures: every valid lane is still attempted, the
recovery package and code never outlive a forget, and no refusal prints a
private path."""
from __future__ import annotations

import contextlib
import importlib.util
import io
import os
from pathlib import Path
import shutil
import stat
import unittest
from unittest import mock

FIXTURE = Path(__file__).with_name("t17-lane-key-forget-test.py")
SPEC = importlib.util.spec_from_file_location("t17_lane_key_forget_fixture", FIXTURE)
assert SPEC and SPEC.loader
base = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(base)
REFUSED = "T17 lane vTPM key forget refused: "


class LaneKeyForgetFailureTest(base.LaneKeyForgetCase):
    def refusals(self, stderr: str) -> list[str]:
        return [line.removeprefix(REFUSED) for line in stderr.splitlines() if line.startswith(REFUSED)]

    def leftovers(self, request: dict) -> list[str]:
        """Recovery files and forget folders still in the lane, relative to it."""
        lane = Path(request["lane_root"])
        return sorted(str(path.relative_to(lane)) for path in lane.rglob("*")
                      if path.name.startswith(("vtpm-recovery", "vtpm-key-forget.")))

    def in_process(self, after_export=None, **patches) -> tuple[int, str]:
        """Run the tool in this process; AFTER_EXPORT(package) runs as soon as each export returns."""
        tool = base.load("t17_lane_key_forget", base.TOOL)
        real = tool.run

        def run(executable, digest, arguments):
            completed = real(executable, digest, arguments)
            if arguments[0] == "export" and after_export:
                after_export(Path(base.options(["--vtpm-lifecycle", *arguments])["--package"]))
            return completed
        with contextlib.ExitStack() as stack:
            for name, value in {"run": run, **patches}.items():
                stack.enter_context(mock.patch.object(tool, name, value))
            stderr = stack.enter_context(contextlib.redirect_stderr(io.StringIO()))
            status = tool.main(["--work", str(self.work), "--verified", str(self.verified)])
        return status, stderr.getvalue()

    def test_every_release_lane_is_attempted_after_another_refuses(self) -> None:
        self.seal("release")
        requests = [self.lane(number) for number in (1, 2, 3)]
        self.key(requests[1], "refuse").touch()
        result = self.forget()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual([self.key(request).exists() for request in requests], [False, True, False],
                         "lanes 1 and 3 must be forgotten although lane 2 refused")
        self.assertEqual([(base.options(call)["--stable-vm-id"], call[1]) for call in self.calls()],
                         [(request["vm_slug"], command) for request in requests for command in ("export", "forget-import")])
        self.assertEqual(self.refusals(result.stderr),
                         ["lane 2: key was not forgotten: forget-import gave no exact proof for its slug"])

    def test_each_refused_lane_reports_its_own_reason(self) -> None:
        self.seal("release")
        requests = [self.lane(1), self.lane(2), self.lane(3, key=False)]
        self.key(requests[0], "refuse").touch()
        result = self.forget()
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertFalse(self.key(requests[1]).exists(), "lane 2 was not attempted after lane 1 refused")
        self.assertEqual(self.refusals(result.stderr), [
            "lane 1: key was not forgotten: forget-import gave no exact proof for its slug",
            "lane 3: key was not exported although its vTPM state holds data"])

    def test_a_refusal_leaves_no_recovery_material_in_the_kept_lane(self) -> None:
        cases = {"forget-import refused": ("data", True, "refuse", ["export", "forget-import"]),
                 "forget-import printed no proof": ("data", True, "silent", ["export", "forget-import"]),
                 "export refused with state present": ("data", False, None, ["export"]),
                 "export unreadable for absent state": ("absent", False, "unreadable", ["export"])}
        for name, (state, key, marker, commands) in cases.items():
            with self.subTest(name):
                shutil.rmtree(self.work / "lane-1", ignore_errors=True)
                (self.keychain / "calls.jsonl").unlink(missing_ok=True)
                request = self.lane(1, state=state, key=key)
                if marker:
                    self.key(request, marker).touch()
                self.assert_refused(self.forget(), commands)
                self.assertEqual(self.leftovers(request), [], "the kept lane holds recovery material or a forget folder")
                self.assertEqual(self.key(request).exists(), key)

    def test_an_export_whose_forget_is_never_reached_leaves_no_material(self) -> None:
        request = self.lane(1)

        def change_the_app(package: Path) -> None:
            with self.executable.open("a") as executable:
                executable.write("# changed after the export\n")
        status, stderr = self.in_process(after_export=change_the_app)
        self.assertEqual(status, 1, stderr)
        self.assertEqual([call[1] for call in self.calls()], ["export"])
        self.assertEqual(self.leftovers(request), [], "the export's package and code outlived an unreached forget")
        self.assertEqual(self.refusals(stderr), ["lane 1: app executable differs from its sealed SHA-256"])
        self.assertTrue(self.key(request).exists())

    @unittest.skipUnless(hasattr(os, "chflags"), "needs BSD file flags")
    def test_material_that_cannot_be_removed_is_named_and_the_proven_outcome_kept(self) -> None:
        request = self.lane(1)

        def pin(package: Path) -> None:
            os.chflags(package, stat.UF_IMMUTABLE)
            self.addCleanup(os.chflags, package, 0)
        status, stderr = self.in_process(after_export=pin)
        self.assertIn("T17 lane vTPM key forget could not remove lane 1 vtpm-recovery.json (PermissionError EPERM)", stderr)
        self.assertEqual(status, 0, stderr)
        self.assertFalse(self.key(request).exists())
        self.assertEqual(sorted(path.name for path in Path(request["lane_root"]).rglob("vtpm-recovery*")), ["vtpm-recovery.json"])
        self.assertNotIn(str(self.temporary), stderr)

    def test_an_os_error_on_a_private_path_is_reported_without_the_path(self) -> None:
        request = self.lane(1)
        lane = Path(request["lane_root"])
        lane.chmod(0)
        self.addCleanup(lane.chmod, 0o700)
        unreadable = self.forget()
        lane.chmod(0o700)
        self.executable.chmod(0o644)
        unlaunchable = self.forget()
        for result, refusal in ((unreadable, "lane 1 could not be read (PermissionError EACCES)"),
                                (unlaunchable, "lane 1: PermissionError EACCES")):
            with self.subTest(refusal):
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertNotIn(str(self.temporary), result.stderr, "a private path reached the tier log")
                self.assertEqual(self.refusals(result.stderr), [refusal])
        self.assertEqual(self.calls(), [], "an app that cannot be launched ran")
        self.assertEqual(self.leftovers(request), [])

    def test_each_export_writes_into_a_new_private_folder_in_its_own_lane(self) -> None:
        self.seal("release")
        requests = [self.lane(number) for number in (1, 2, 3)]
        folders = []
        status, stderr = self.in_process(after_export=lambda package: folders.append(
            (package.parent.parent, package.parent.name.startswith("vtpm-key-forget."), stat.S_IMODE(package.parent.lstat().st_mode))))
        self.assertEqual(status, 0, stderr)
        self.assertEqual(folders, [(Path(request["lane_root"]), True, 0o700) for request in requests])

    def test_a_command_that_does_not_return_fails_closed(self) -> None:
        request = self.lane(1)
        self.key(request, "hang").touch()
        status, stderr = self.in_process(TIMEOUT_SECONDS=1)
        self.assertEqual(status, 1)
        self.assertIn("did not finish", stderr)
        self.assertTrue(self.key(request).exists())


if __name__ == "__main__":
    unittest.main()
