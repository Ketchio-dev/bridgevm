"""Headless tip-coordinate freshness and click sequencing contracts."""
import os
import pathlib
import subprocess
import tempfile
import unittest

HELPER = pathlib.Path(__file__).resolve().parents[2] / "scripts/b6-tip-dismiss.sh"


class TipContracts(unittest.TestCase):
    def invoke(self, response, stale="", command="b6_tip_hid_point 42", failed=False):
        with tempfile.TemporaryDirectory() as directory:
            log, control = pathlib.Path(directory) / "run.log", pathlib.Path(directory) / "input.ctl"
            log.write_text(stale)
            control.write_text("")
            env = dict(os.environ, RUN_LOG=str(log), INPUT=str(control), WIDTH="1600", HEIGHT="900",
                       RESPONSE=response, FAILED="1" if failed else "0", HELPER=str(HELPER), COMMAND=command)
            result = subprocess.run(["bash", "-c", '''
source "$HELPER"
send_ok() { [[ "$FAILED" == 0 ]] || return 1; [[ -z "$RESPONSE" ]] || printf '%s\n' "$RESPONSE" >> "$RUN_LOG"; return 0; }
wait_baseline() { printf 0; }
wait_after() { return 0; }
sleep() { :; }
eval "$COMMAND"
'''], env=env, capture_output=True, text=True)
            return result, control.read_text()

    def test_owned_physical_point(self):
        result, _ = self.invoke("BVTIPPOINT hwnd=42 state=present x=624 y=258 owner=42\r")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, f"{624 * 32767 // 1599}x{258 * 32767 // 899}")

    def test_not_found_never_clicks(self):
        result, control = self.invoke("BVTIPPOINT hwnd=42 state=not-found", command="b6_dismiss_packaged_tip 42")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(control, "")

    def test_still_present_clicks_once_then_fails(self):
        result, control = self.invoke("BVTIPPOINT hwnd=42 state=present x=624 y=258 owner=42", command="b6_dismiss_packaged_tip 42")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(control.count("POINTER click:"), 1)

    def test_refuses_bad_and_stale_records(self):
        valid = "BVTIPPOINT hwnd=42 state=present x=624 y=258 owner=42"
        for value in (valid.replace("owner=42", "owner=43"), valid.replace("hwnd=42", "hwnd=43"),
                      valid.replace("x=624", "x=1600"), valid.replace("y=258", "y=-1"), valid + "\n" + valid, ""):
            with self.subTest(value=value):
                result, _ = self.invoke(value, stale=valid + "\n")
                self.assertNotEqual(result.returncode, 0)

    def test_failed_guest_command_never_clicks(self):
        result, control = self.invoke("BVTIPPOINT hwnd=42 state=present x=624 y=258 owner=42", command="b6_dismiss_packaged_tip 42", failed=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(control, "")


if __name__ == "__main__":
    unittest.main()
