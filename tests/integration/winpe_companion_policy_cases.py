"""Exercise the real macOS launcher parser without starting a VM."""
import os
import platform
import subprocess

from winpe_companion_size_cases import assert_vars_sizes


def assert_wrapper_policy(case, command):
    case.assertIn("--skip-build", command)
    case.assertEqual(command[command.index("--watchdog-ms") + 1], "300000")
    case.assertTrue(command[command.index("--display-export-ppm") + 1].endswith("/latest.ppm"))
    samples = command[command.index("--ramfb-samples") + 1]
    case.assertTrue(all(0 < int(value) <= 120000 for value in samples.split(",")))
    if platform.system() != "Darwin":
        return  # The dedicated hosted workflow uses macOS for the actual parser.
    environment = {key: value for key, value in os.environ.items() if not key.startswith("BRIDGEVM_")}
    valid = subprocess.run(command + ["--print-policy"], env=environment,
                           capture_output=True, text=True, timeout=30)
    case.assertEqual(valid.returncode, 0, valid.stdout + valid.stderr)
    invalid = list(command)
    invalid[invalid.index("--ramfb-samples") + 1] = "240000,290000"
    refused = subprocess.run(invalid + ["--print-policy"], env=environment,
                             capture_output=True, text=True, timeout=30)
    case.assertEqual(refused.returncode, 2, refused.stdout + refused.stderr)
    case.assertIn("each <= 120000", refused.stderr)
