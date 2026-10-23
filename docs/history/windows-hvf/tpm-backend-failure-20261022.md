# TPM backend failure fails closed — 2026-10-22

Classification: robustness fix for two defects that a read-only review found on
2026-10-22, and a fix for a regression found on 2026-10-23 (last section). The
evidence is automated tests and static reasoning; only the regression test ran
a real swtpm process. No live run, Windows guest or BitLocker volume has
exercised any of the changes. No criterion or product state changes.

## Findings, verified in code at d1f17b2d

- **Reset panic:** `VirtPlatform::reset` in
  `crates/bridgevm-hvf/src/platform_virt/machine_assembly.rs` called
  `tpm.reset().expect("TPM backend reset failed")`. The probe's PSCI
  SYSTEM_RESET arm runs it with the platform mutex held. A rejected or broken
  swtpm CMD_INIT therefore panicked the VMM and poisoned that mutex. This is
  the in-process reboot path, which probe runs use without
  `BRIDGEVM_EXIT_ON_RESET` (8 reboots by default). Product runs set that flag
  in `crates/bridgevm-hvf-runtime/src/vm_process.rs`, so a guest reset
  recreates the helper and never reaches this path.
- **Misaligned responses:** the raw swtpm backend sets 30 s read and write
  timeouts on both sockets. After a timeout, a short read or a response size
  outside 10..=4096 bytes, the failing command already got `TPM_RC_FAILURE`.
  The next command, however, wrote to and read from the same stream and so
  received the unread rest of the earlier response. That rest can be a
  well-formed success for another command. This applies to every run that
  attaches swtpm, product runs included.

## Chosen behaviour

- **Data or control socket failure:** `SwtpmUnixBackend`, now in
  `crates/bridgevm-hvf/src/tpm_swtpm.rs`, latches the first failure on
  either socket: an I/O error (including a timeout, a short read or a broken
  pipe) or a response size outside the TIS buffer. Neither socket is read or
  written again. The failing command and every later one complete through the
  TIS FIFO with `80 01 00 00 00 0a 00 00 01 01` (TPM_ST_NO_SESSIONS, size 10,
  TPM_RC_FAILURE), so the guest's TIS state machine completes. One stderr line
  names the cause, for example "TPM2 swtpm backend failed closed on the data
  socket: timed out: Resource temporarily unavailable (os error 35)".
  - A command refused before any socket I/O does not latch: a locality other
    than 0, or a malformed command. The stream is still paired, and a guest
    cannot disable its TPM for the rest of the run that way.
  - No success response is ever fabricated, and no value comes from time or
    process identity.
- **Reset failure:** the run stops.
  - `VirtPlatform::reset` power-cycles the TPM first and returns the error
    before any device is reset, so the final report still sees the
    generation that asked for the reset.
  - The reboot arm's `system_reset_failure`, in
    `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime/system_reset_failure.rs`,
    reuses the existing `hv_gic_reset` failure path. It sets
    `fatal_reset_error` and prints
    `stop: TPM backend reset failed during PSCI SYSTEM_RESET: <cause>`, and
    the probe exits with status 1, which the supervisor reports as a helper
    failure.
  - A latched backend refuses the reset too, without sending CMD_INIT,
    because CMD_INIT cannot drain the data socket.
- **Why stop instead of booting with a failing TPM:** after a failed power
  cycle, swtpm may still hold the previous boot's PCRs, sessions and platform
  authorization, so answering from it is unsafe. Answering only failures
  boots a guest whose measured boot fails and whose BitLocker protector may
  ask for recovery, with nothing on the host to explain why. Stopping hands
  the guest no TPM data and leaves a host diagnostic, and the guest had
  already asked to reset. A fresh start reconnects to swtpm.

## QEMU comparison

These points come from memory of QEMU's `backends/tpm/tpm_emulator.c`,
`backends/tpm/tpm_util.c` and `hw/tpm/tpm_tis_common.c`. They were not re-read
for this change.

| QEMU behaviour | Confidence |
| --- | --- |
| A failed exchange gets `tpm_util_write_fatal_error_response`: 10 bytes in TPM 1.2 format, with tag 0x00C4 and TPM_FAIL (9) | high |
| The emulator data socket has no read timeout, and the next command reuses it without a failed latch | moderate |
| `tpm_tis_reset` reports a failed `tpm_backend_startup_tpm` (CMD_INIT) and calls `exit(1)` | moderate to high |

BridgeVM's stop on a failed reset therefore matches QEMU's outcome. The latch,
the TPM 2.0 failure code and the stop at the next reboot after a latched data
failure are deviations. They are recorded as `vtpm-backend-failure` in
`docs/machine-contract/qemu-virt-deviations-vtpm-failure.json`. Until now the
registry described TPM lifecycle only, in the `vtpm-backend` entry.

## Tests

For (a), "before" is d1f17b2d with the backend moved unchanged into
`tpm_swtpm.rs` and a `from_streams` constructor added. For (b), "before" is
22112049 with `VirtPlatform::reset` returning `Result` but still panicking.
Every test ran on the 1.97.0 toolchain.

| Commit | Test | Before | After |
| --- | --- | --- | --- |
| 22112049 | `timeout_mid_response_fails_this_and_later_commands_without_stale_data` | the second command got the stale success `80 01 00 00 00 0e 00 00 00 00 00 02 a5 5a` | both commands fail; swtpm saw 1 command |
| 22112049 | `out_of_range_response_size_fails_this_and_later_commands_without_stale_data` (sizes 4097 and 9) | the same stale success | both fail; 1 command |
| 22112049 | `failed_cmd_init_refuses_later_commands_and_resets` | a command after the rejected CMD_INIT returned success | fails; no command and no second CMD_INIT reach swtpm |
| 22112049 | `reset_after_data_failure_is_refused_without_cmd_init` | the reset succeeded | the reset fails; no CMD_INIT |
| 22112049 | `refused_locality_does_not_latch_the_backend` | pass (guard) | pass |
| a3da88bd | `tpm_reset_failure_is_returned_before_any_device_is_reset` | panic at machine_assembly.rs:286 | error returned; UART output kept |
| a3da88bd | `tpm_reset_failure_stops_the_run_without_panicking_or_poisoning_the_platform` | panic, poisoned mutex | stop reason, exit FAILURE |
| a3da88bd | `gic_reset_failure_stops_before_the_platform_is_reset`, `successful_resets_let_the_reboot_continue` | pass (guards) | pass |

- The swtpm tests in `crates/bridgevm-hvf/src/tpm_swtpm_tests.rs` use
  scripted peers over socket pairs. They went from 3 passed and 4 failed to 7
  passed, a count that includes the 2 moved reset tests.
- The platform test is in
  `crates/bridgevm-hvf/src/platform_virt/tests/tpm_reset_failure.rs`. The
  probe tests are beside `system_reset_failure.rs`; they went from 2 passed
  and 1 failed to 3 passed.
- At a3da88bd the bridgevm-hvf lib suite passes 1463 tests with 2 ignored,
  and the `hvf_gic_boot_probe` example suite passes 633.
- These checks pass: clippy with `-D warnings`, rustfmt, the structural
  budgets, and the deviation schema and module checks. `tpm_tis.rs` went from
  815 to 620 counted lines and `probe_runtime.rs` from 1000 to 997, and both
  ceilings were lowered to match.

## Not verified

- No real swtpm process ran for these two fixes. Timeouts, short reads and
  rejected CMD_INIT results are simulated with socket pairs and injected
  backends.
- No guest ran. How EDK2, Windows TPM.sys or BitLocker react to
  `TPM_RC_FAILURE` after a latch is untested.
- The QEMU comparison is recalled, not re-read.
- Not examined (static reading only): product runs keep one swtpm process
  across helper generations (`runners/hvf-runner/src/launch_spec_execution.rs`),
  and a new helper sends no CMD_INIT when it connects. Whether swtpm's
  volatile state is reset across a product reboot was not checked. It was
  checked on 2026-10-23: it is not, which was a regression (next section).

## Product reboots did not power-cycle the vTPM — 2026-10-23

### Regression

- Since 6ab445b7 (2026-08-07), `crates/bridgevm-hvf-runtime/src/vm_process.rs`
  starts every product helper with `BRIDGEVM_EXIT_ON_RESET=1`. A guest reset
  ends the helper with exit 42, and `run_prepared_vm` in
  `crates/bridgevm-hvf-runtime/src/controlled_run.rs` starts the next helper
  against the same swtpm sockets.
- `runners/hvf-runner/src/launch_spec_execution.rs` starts one swtpm per
  execute, with `--flags not-need-init,startup-clear` from
  `crates/bridgevm-hvf-runtime/src/controlled_swtpm_command.rs`.
- The probe connected with `SwtpmUnixBackend::connect_with_control` and sent
  no CMD_INIT. The only CMD_INIT was in the backend's `reset()`, which only
  the in-process `VirtPlatform::reset` reaches, and product runs never reboot
  in process.
- swtpm keeps volatile TPM state across client reconnects. After a product
  reboot the TPM therefore still held the previous boot's PCRs, and the next
  `TPM2_Startup(CLEAR)` failed with TPM_RC_INITIALIZE (0x100).
- Product reboots so lacked the power cycle that fixed defect 1 of the
  [PPI-clear receipt](../../windows-arm/evidence/vtpm-windows-ppi-clear-20260722.md).
  That fix put CMD_INIT on the in-process reset path, and that run rebooted
  in process.

### Evidence and its rank

- Rank 3, automated test against a real swtpm:
  `a_new_helper_generation_boots_from_cleared_pcrs` starts swtpm 0.10.1
  through the runtime's `start_swtpm`, which uses the product argv.
  - As generation 1 it sends `TPM2_Startup(CLEAR)`, extends PCR 0 and drops
    both connections.
  - As generation 2 it connects through the probe's boot setup and sends
    `TPM2_Startup(CLEAR)`.
  - Before the fix, that Startup returned 0x100 and PCR 0 still held
    generation 1's extend.
- Rank 3: an earlier host experiment outside the repository, a Python client
  against swtpm with the same flags, showed the same carry-over. It also
  showed that CMD_INIT followed by `TPM2_Startup(CLEAR)` reads PCR 0 as zero.
- Rank 4, static reasoning that nothing has observed: with PCRs and platform
  authorization carried over a product restart, a BitLocker TPM protector
  would fail after a guest restart but not after a cold start, and a PPI clear
  would again fail with TPM_RC_BAD_AUTH, as in defect 1.

### Fix (acae6e96)

- `SwtpmUnixBackend::connect_for_boot`, in
  `crates/bridgevm-hvf/src/tpm_swtpm_boot.rs`, connects and, when a control
  socket is configured, power-cycles through the latched `reset()`. Without a
  control socket it sends nothing.
- `generation_tpm`, in
  `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime/generation_tpm.rs`,
  runs from `prepare()` in `probe_media_lease.rs`, before `create_vm`.
  - With a control socket, every generation sends exactly one CMD_INIT
    before any guest instruction, and prints
    `TPM2 TIS backend: power-cycled with CMD_INIT before boot`.
  - Every in-process reboot still sends one through `VirtPlatform::reset`.
- A rejected or failed CMD_INIT, or a failed swtpm connect, prints
  `stop: TPM backend setup failed before the first guest instruction: <cause>`
  and the helper exits with status 1 before the VM exists, so no final report
  is printed. As with the reset-failure stop, the supervisor reports "VM
  helper failed". A failed connect used to panic after VM creation.
- In restore mode (`BRIDGEVM_RESTORE_STATE`, read in
  `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/checkpoint_glue.rs`), the
  backend connects without CMD_INIT, because the restored guest is already
  running. The probe prints
  `TPM2 TIS backend: checkpoint restore, no power cycle`.

### Restore mode and checkpoints: a pre-existing limitation

- `BRIDGEVM_RESTORE_STATE` is read only by `checkpoint_glue.rs`, and only
  `scripts/run-hvf-windows-installed-boot-runner.sh` passes it through.
  Neither `crates/bridgevm-hvf-runtime/src/vm_process.rs` nor
  `vm_process_surfaces.rs` passes it to the product helper, so product runs
  cannot restore a checkpoint.
- Checkpoints carry no TIS or swtpm state. Neither
  `crates/bridgevm-hvf/src/platform_virt/snapshot.rs` nor
  `crates/bridgevm-hvf/src/checkpoint/` mentions the TPM. A restored guest
  therefore sees whatever swtpm holds at restore time. This limitation
  predates this work and is unchanged.

### QEMU comparison

This point comes from memory of QEMU's `hw/tpm/tpm_tis_common.c` and
`backends/tpm/tpm_emulator.c`, not re-read. `tpm_tis_reset` runs on every
machine reset, including the initial reset before the guest first runs. It
calls `tpm_backend_startup_tpm`, which sends the emulator CMD_INIT. Confidence:
moderate to high.

With the fix, reset semantics match QEMU's: the first boot, a product reboot
and an in-process reboot each power-cycle the TPM once before the guest runs.
Before the fix, the deviation registry did not record the product difference.
The `vtpm-backend-failure` entry now records both.

### Tests

Here "before" means acae6e96's tests run against a `connect_for_boot` that only
connected, which was the shipped behaviour.

| Test | Before | After |
| --- | --- | --- |
| `a_booting_generation_power_cycles_once_before_its_first_tpm_command` | swtpm saw the command and no CMD_INIT | one CMD_INIT, then the command |
| `a_rejected_power_cycle_returns_the_stop_line_before_any_tpm_command` | setup succeeded, so the helper would boot | the stop line; swtpm saw only the CMD_INIT |
| `without_a_control_socket_no_power_cycle_is_sent`, `a_checkpoint_restore_connects_without_a_power_cycle`, `without_a_data_socket_there_is_no_tpm` | pass (guards) | pass |
| `a_new_helper_generation_boots_from_cleared_pcrs` (real swtpm 0.10.1) | generation 2's Startup returned 0x100 and PCR 0 kept generation 1's extend | PCR 0 reads zero; 1 of 1, then 5 of 5 repeats |

- The first five tests are beside `generation_tpm.rs`, and they use scripted
  swtpm peers on Unix sockets. They went from 3 passed and 2 failed to 5
  passed.
- The real-swtpm test is in
  `crates/bridgevm-hvf-runtime/src/vtpm_generation_tests_real.rs`.
  - Like the existing real-swtpm tests, it skips when
    `/opt/homebrew/bin/swtpm` is absent.
  - Only the release workflow installs swtpm, so ordinary hosted CI is
    expected to skip it.
- At acae6e96 the bridgevm-hvf lib suite passes 1463 tests with 2 ignored,
  the `hvf_gic_boot_probe` example suite 638, and the bridgevm-hvf-runtime
  suite 67 with 1 ignored. Clippy with `-D warnings`, rustfmt, the structural
  budgets, the reset-record and stop-line contracts and the vTPM live-evidence
  verifier smoke pass.

### Not verified

- No Windows guest, BitLocker volume or PPI clear has run, with or without
  the fix. The guest effects above are static reasoning.
- No product run has exercised the helper respawn. The real-swtpm test plays
  both generations in one test process.
- The claim that a failed CMD_INIT stops the helper before any vCPU runs rests
  on the unit test and on `prepare()` running before `create_vm` in
  `probe_runtime.rs`. No helper run has shown it.
- The QEMU comparison is recalled, not re-read.
