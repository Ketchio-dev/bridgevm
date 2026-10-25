"""The retained T19 source's vTPM state copy and other failures print only fixed refusal text.

The tier runs retain-windows-import-source.py with its stdout and stderr in the
log, where only booleans, sizes, hashes and fixed tokens may appear. /bin/cp
names the state and staging paths in its errors, and the text of an OS error or
of a helper process error names paths too. These cases reuse the export cases'
fixture and run main() the same way, with file descriptors 1 and 2 captured.
"""
from __future__ import annotations

import os
import subprocess
import unittest

from retained_windows_export_cases import RETAIN, WRITE, RetainedExportOutputCases  # noqa: F401

REFUSED = "T17-to-T19 handoff refused: "


class RetainedRefusalOutputCases(unittest.TestCase):
    # The export cases' fixture, without their tests: a stub packaged app,
    # synthetic media and a private vTPM state.
    setUp = RetainedExportOutputCases.setUp
    tearDown = RetainedExportOutputCases.tearDown
    export_selected = RetainedExportOutputCases.export_selected
    retain = RetainedExportOutputCases.retain

    def assert_refused(self, status: int, output: str, text: str):
        self.assertEqual((status, output), (1, REFUSED + text + "\n"))
        self.assertFalse(os.path.lexists(self.args.destination) or os.path.lexists(self.args.status))
        self.assertEqual(list(self.args.destination.parent.glob(".source.stage-*")), [])

    def test_failed_state_copy_prints_only_a_fixed_token_with_its_exit_status(self):
        # The export runs after the state was hashed; an unreadable file then makes cp fail naming its path.
        status, output = self.retain(WRITE + f'chmod 000 "{self.state}/state.bin"\n')
        self.assert_refused(status, output, "vTPM state copy failed with exit status 1")

    def test_an_os_error_is_reported_by_its_errno_name_only(self):
        # The export takes the state copy's directory name, so creating it raises FileExistsError naming it.
        status, output = self.retain(WRITE + 'mkdir "${package%/*}/vtpm"\n')
        self.assert_refused(status, output, "operating-system error (EEXIST)")

    def test_a_helper_process_error_is_reported_by_its_class_only(self):
        def hung_helper(request, authenticated, staging):
            raise subprocess.TimeoutExpired([str(self.app / "snapshot_pair_cli"), "create", str(staging)], 1800)
        self.export_selected = hung_helper
        status, output = self.retain(WRITE)
        self.assert_refused(status, output, "helper process error (TimeoutExpired)")
