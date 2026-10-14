"""The extracted observer parser preserves arguments, limits and private output."""
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import Mock, patch

import a19_interrupt_child_cli as cli


class CLIArgumentCases:
    def test_observer_cli_defaults_and_declared_stop_point(self):
        for flags, deadline, point in (([], 300, "staged-disk-verify-read"),
                                      (["--deadline", "1", "--stop-point", "swap-staged-disk-verify-read"],
                                       1, "swap-staged-disk-verify-read")):
            with self.subTest(flags=flags), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                run = Mock(return_value={"mock_observation": True})
                argv = ["observer", "helper", "snapshot", "disk", "vars", str(output)] + flags
                with patch.object(sys, "argv", argv):
                    self.assertEqual(cli.main(run), 0)
                run.assert_called_once_with(Path("helper"), Path("snapshot"), Path("disk"),
                                            Path("vars"), output, deadline, point)
                receipt = output / "interrupt-observation.json"
                self.assertEqual(json.loads(receipt.read_text()), run.return_value)
                self.assertEqual(receipt.stat().st_mode & 0o777, 0o600)

    def test_observer_cli_refuses_invalid_limits_before_dispatch(self):
        for flags in (["--deadline", "0"], ["--deadline", "301"], ["--stop-point", "undeclared"]):
            with self.subTest(flags=flags):
                run = Mock()
                with patch.object(sys, "argv", ["observer", "h", "s", "d", "v", "out"] + flags), \
                        contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
                    cli.main(run)
                self.assertEqual(raised.exception.code, 2)
                run.assert_not_called()

    def test_observer_cli_preserves_run_exception_without_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            error = RuntimeError("original observation failure")
            with patch.object(sys, "argv", ["observer", "h", "s", "d", "v", temporary]), \
                    self.assertRaises(RuntimeError) as raised:
                cli.main(Mock(side_effect=error))
            self.assertIs(raised.exception, error)
            self.assertFalse((Path(temporary) / "interrupt-observation.json").exists())
