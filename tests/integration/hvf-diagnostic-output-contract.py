#!/usr/bin/env python3
"""Exercise actual bounded launcher plumbing with disposable emitters, never HVF."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from bounded_output import read_status


class LauncherBoundsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)

    def shell(self, body, *, env=None):
        prefix = 'set -euo pipefail\nROOT="$1"; EVIDENCE_DIR="$2"\nsource "$ROOT/scripts/run-hvf-windows-installed-boot-runner.sh"\n'
        return subprocess.run(["/bin/bash", "-c", prefix + body, "_", str(ROOT), str(self.path)],
                              env=env, capture_output=True, timeout=15)

    def test_direct_native_pid_and_nonzero_exit_survive_logger(self):
        executable = self.path / "emitter"
        executable.write_text("#!/usr/bin/env python3\nimport os,sys\nprint(os.getpid(),flush=True)\nos.write(1,b'\\xffbytes')\nsys.exit(7)\n")
        executable.chmod(0o700)
        result = self.shell('''
DIAGNOSTIC_OUTPUT_BOUNDS=1; BIN="$EVIDENCE_DIR/emitter"
ENV_ARGS=(BRIDGEVM_OWNED_FIXTURE=1); HOST_PAUSE_RESUME_PROOF_MS=1
prepare_virtio_gpu_trace() { :; }
host_pause_resume_control_path() { printf '%s/control' "$EVIDENCE_DIR"; }
drive_host_pause_resume_proof() { printf '%s' "$PROBE_PID" > "$EVIDENCE_DIR/direct-pid"; }
run_probe_process
[[ "$RUN_STATUS" == 7 && -z "$PROBE_PID" && -z "$BOUNDED_LOGGER_PID" ]]
[[ ! -e "$EVIDENCE_DIR/run.log.pipe" ]]
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        data = (self.path / "run.log").read_bytes()
        self.assertEqual(data, (self.path / "direct-pid").read_bytes() + b"\n\xffbytes")
        read_status(self.path / "run-log-bound.json", 512 * 1024 * 1024, output=self.path / "run.log")

    def test_exact_limit_and_one_byte_overflow_are_distinct(self):
        for count in (16, 17):
            with self.subTest(count=count):
                result = self.shell('''
start_bounded_file "$EVIDENCE_DIR/out" "$EVIDENCE_DIR/status" 16
python3 -c 'import os; os.write(1,b"x"*int(os.environ["COUNT"]))' > "$BOUNDED_FIFO" 9>&-
set +e
finish_bounded_file
printf '%s' "$?" > "$EVIDENCE_DIR/result"
''', env=dict(os.environ, COUNT=str(count)))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((self.path / "out").read_bytes(), b"x" * 16)
                value = json.loads((self.path / "status").read_text())
                self.assertEqual((value["observed_bytes"], value["complete"], value["overflow"]),
                                 (count, count == 16, count == 17))
                self.assertEqual((self.path / "result").read_text(), "0" if count == 16 else "1")
                for path in self.path.iterdir(): path.unlink()

    def test_old_ready_status_refuses_launch_and_preserves_colliding_output(self):
        from bounded_output import BoundedOutput
        with BoundedOutput(self.path / "run.log", self.path / "run-log-bound.json", 512 * 1024 * 1024) as output:
            stale = (self.path / "run-log-bound.json").read_bytes()
        (self.path / "run-log-bound.json").write_bytes(stale)
        original = {p: p.read_bytes() for p in self.path.iterdir()}
        result = self.shell('''
DIAGNOSTIC_OUTPUT_BOUNDS=1; BIN=/usr/bin/false
ENV_ARGS=(BRIDGEVM_OWNED_FIXTURE=1); HOST_PAUSE_RESUME_PROOF_MS=""
prepare_virtio_gpu_trace() { :; }
run_probe_process
[[ "$RUN_STATUS" == 1 && -z "$BOUNDED_LOGGER_PID" ]]
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(original, {p: p.read_bytes() for p in original})
        self.assertTrue((self.path / "output-bound-refused.json").is_file())

    def test_unowned_fd9_is_preserved_without_fifo_or_logger(self):
        result = self.shell('''
exec 9> "$EVIDENCE_DIR/sentinel"
if start_bounded_file "$EVIDENCE_DIR/out" "$EVIDENCE_DIR/status" 16; then exit 1; fi
printf kept >&9
exec 9>&-
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.path / "sentinel").read_bytes(), b"kept")
        self.assertEqual({p.name for p in self.path.iterdir()}, {"sentinel", "output-bound-refused.json"})

    def test_logger_interpreter_does_not_honor_path_override(self):
        tools = self.path / "tools"; tools.mkdir()
        python = tools / "python3"
        python.write_text('#!/bin/bash\nprintf invoked > "$POISON_MARKER"\nexit 99\n')
        python.chmod(0o700)
        environment = dict(os.environ, PATH=str(tools) + ":" + os.environ["PATH"],
                           POISON_MARKER=str(self.path / "poison-used"))
        result = self.shell('''
start_bounded_file "$EVIDENCE_DIR/out" "$EVIDENCE_DIR/status" 16
printf full-bytes > "$BOUNDED_FIFO" 9>&-
finish_bounded_file
''', env=environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.path / "poison-used").exists())
        self.assertEqual((self.path / "out").read_bytes(), b"full-bytes")

    def test_native_output_cannot_truncate_a_replacement_fifo_path(self):
        executable = self.path / "emitter"
        executable.write_text("#!/bin/bash\nprintf '%s' native-output\n")
        executable.chmod(0o700)
        result = self.shell('''
eval "$(declare -f start_bounded_file | sed '1s/start_bounded_file/acquire_bounded_file/')"
start_bounded_file() {
  acquire_bounded_file "$@" || return
  rm -- "$BOUNDED_FIFO"
  printf retained-sentinel > "$BOUNDED_FIFO"
}
DIAGNOSTIC_OUTPUT_BOUNDS=1; BIN="$EVIDENCE_DIR/emitter"
ENV_ARGS=(BRIDGEVM_OWNED_FIXTURE=1); HOST_PAUSE_RESUME_PROOF_MS=""
prepare_virtio_gpu_trace() { :; }
run_probe_process
[[ "$RUN_STATUS" == 1 ]]
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.path / "run.log.pipe").read_bytes(), b"retained-sentinel")
        self.assertEqual((self.path / "run.log").read_bytes(), b"native-output")
        self.assertTrue((self.path / "output-bound-refused.json").is_file())

    def test_cleanup_sink_preserves_original_failure_status(self):
        result = self.shell('''
DIAGNOSTIC_OUTPUT_BOUNDS=1; PROBE_PID=""; SWTPM_PID=""
terminate_owned_probe() { printf 'owned probe cleanup\\n'; }
terminate_owned_swtpm() { :; }; cleanup_owned_swtpm_runtime() { :; }
trap cleanup EXIT
exit 7
''')
        self.assertEqual(result.returncode, 7, result.stderr)
        value = read_status(self.path / "cleanup-bound.json", 16 * 1024 * 1024,
                            output=self.path / "cleanup.txt")
        self.assertTrue(value["complete"])
        self.assertIn(b"cleanup_status=7", (self.path / "cleanup.txt").read_bytes())

    def test_replaced_fifo_is_preserved_and_refused_in_finish_and_unwind(self):
        root = self.path
        for unwind in (False, True):
            with self.subTest(unwind=unwind):
                self.path = root / str(unwind); self.path.mkdir()
                body = '''
DIAGNOSTIC_OUTPUT_BOUNDS=1; PROBE_PID=""; SWTPM_PID=""
terminate_owned_probe() { :; }; terminate_owned_swtpm() { :; }; cleanup_owned_swtpm_runtime() { :; }
start_bounded_file "$EVIDENCE_DIR/out" "$EVIDENCE_DIR/status" 16
rm -- "$BOUNDED_FIFO"
printf replacement > "$BOUNDED_FIFO"
stat -f '%i' "$BOUNDED_FIFO" > "$EVIDENCE_DIR/replacement-inode"
'''
                body += 'trap cleanup EXIT\nexit 0\n' if unwind else 'finish_bounded_file\n'
                result = self.shell(body)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                path = self.path / "out.pipe"
                self.assertEqual(path.read_bytes(), b"replacement")
                self.assertEqual(path.stat().st_ino, int((self.path / "replacement-inode").read_text()))
                self.assertTrue((self.path / "output-bound-refused.json").is_file())
                for path in self.path.iterdir(): path.unlink()

    def test_outer_timeout_refuses_without_live_logger_or_native(self):
        from winpe_companion_process import OwnedProcessError, run_owned
        executable = self.path / "emitter"
        executable.write_text("#!/usr/bin/env python3\nimport os,time\nos.write(1,b'owned-start\\n')\ntime.sleep(30)\n")
        executable.chmod(0o700)
        body = '''set -euo pipefail
ROOT="$1"; EVIDENCE_DIR="$2"
source "$ROOT/scripts/run-hvf-windows-installed-boot-output.sh"
DIAGNOSTIC_OUTPUT_BOUNDS=1; BIN="$EVIDENCE_DIR/emitter"
ENV_ARGS=(BRIDGEVM_OWNED_FIXTURE=1); HOST_PAUSE_RESUME_PROOF_MS=1
prepare_virtio_gpu_trace() { :; }
host_pause_resume_control_path() { printf '%s/control' "$EVIDENCE_DIR"; }
drive_host_pause_resume_proof() { printf '%s\n%s\n' "$PROBE_PID" "$BOUNDED_LOGGER_PID" > "$EVIDENCE_DIR/owned-pids"; }
run_probe_process
'''
        with self.assertRaises(OwnedProcessError) as failure:
            run_owned(["/bin/bash", "-c", body, "_", str(ROOT), str(self.path)], timeout=2,
                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # Darwin can retain zombie group membership after TERM; uncertainty is a refusal.
        self.assertIs(type(failure.exception.cleanup_complete), bool)
        self.assertLessEqual((self.path / "run.log").stat().st_size, 512 * 1024 * 1024)
        with self.assertRaises(ValueError):
            read_status(self.path / "run-log-bound.json", 512 * 1024 * 1024)
        for pid in (self.path / "owned-pids").read_text().splitlines():
            state = subprocess.run(["ps", "-p", pid, "-o", "stat="], capture_output=True, timeout=5).stdout.strip()
            self.assertTrue(not state or state.startswith(b"Z"), (pid, state))

    def test_preset_preserves_d4_geometry_and_refuses_other_artifact_sources(self):
        prefix = '''
source "$ROOT/scripts/run-hvf-windows-installed-boot-validation.sh"
source "$ROOT/scripts/run-hvf-windows-installed-boot-args.sh"
init_installed_boot_defaults
parse_installed_boot_args --diagnostic-output-bounds --no-guest-disk-harvest --ramfb-samples 1000,15000,30000,60000,90000,110000,120000
'''
        result = self.shell(prefix + '''
validate_installed_boot_option_combinations
[[ "$RAMFB_SAMPLES" == 1000,15000,30000,60000,90000,110000,120000 ]]
[[ -z "${BRIDGEVM_DUMP_ON_RESET:-}" && -z "${BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64:-}" ]]
[[ "$BRIDGEVM_PREBUILT_PROBE" == owned-binary ]]
build_installed_boot_env_args
printf '%s\\n' "${ENV_ARGS[@]}"
''', env=dict(os.environ, BRIDGEVM_DUMP_ON_RESET="1", BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64="poison",
                    BRIDGEVM_PREBUILT_PROBE="owned-binary"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(b"BRIDGEVM_DIAGNOSTIC_OUTPUT_BOUNDS=1", result.stdout)
        for variable in ("VIRTIO_GPU_3D=1", "BOOT_TIMER=1", "INPUT_CONTROL=owned", "DISPLAY_EXPORT_FB=owned",
                         "GUEST_DISK_HARVEST=1", "SHUTDOWN_AFTER_AGENT_READY=1", "AGENT_SERVICE_CONTROL=owned", "VTPM_STATE_DIR=owned", "HOST_PAUSE_RESUME_PROOF_MS=1"):
            refused = self.shell(prefix + variable + "\nvalidate_installed_boot_option_combinations\n")
            self.assertEqual(refused.returncode, 2, refused.stderr)
            self.assertIn(b"diagnostic output bounds require", refused.stderr)


if __name__ == "__main__":
    unittest.main()
