# App guest-reset detection keyed on runtime records — deterministic

Classification: deterministic development evidence (local automated tests,
evidence rank 3). No physical run has exercised this change. Capability
wording and product state come from `capabilities/windows-hvf.json`; nothing
here promotes a criterion.

## Defect

The [first-logon provisioner and shutdown-gate note](a9-firstboot-provisioner-and-shutdown-gate-20260929.md)
left one defect open: app input and window-inventory paths detected a guest
restart by `PSCI_SYSTEM_RESET`, which the runtime never prints. The five
ordered-input detectors (Unicode text, capability query, acknowledged,
negotiated and router streams) matched only that literal. Window inventory,
its controller and clipboard paste matched any line that starts with
`PSCI SYSTEM_RESET:`. The WINLIST coherence gate matched both.

On a guest SYSTEM_RESET the helper prints one of three records:

- `stop: PSCI 0x84000009 exiting for process recreation (exit 42)` on product
  runs, which set `BRIDGEVM_EXIT_ON_RESET=1`; the next helper generation then
  appends to the same run.log;
- `PSCI SYSTEM_RESET: reboot {n}/{max}` for an in-process reboot;
- `stop: PSCI 0x84000009 max reboot count {max} reached` at the reboot limit,
  which the `--max-reboots 0` input gate hits.

None of the eight app detectors matched the product record, and the gate
missed the limit record. A request pending at a product reset stayed pending
until the next generation's `BVAGENT READY`, or until its 30 s deadline.

## Change

`HvfGuestResetRecord` (Swift) and `scripts/live-gates/hvf_reset_record.py`
define the three records and match whole lines only, ignoring one trailing CR.
`tests/integration/hvf-reset-record-contract.py` rebuilds them from the Rust
format strings and requires every detector and the four standalone `swiftc`
builds that compile detector sources to use the definition. The agent markers
(`BVAGENT READY`, `re-READY`, `SERVICE start`) are unchanged.

Behaviour that changes:

- Each of the three records now cancels pending input, capability, WINLIST and
  paste requests on the poll that reads it. Before, only the in-process reboot
  line cancelled, and only WINLIST and paste.
- An activated router now fails at the record, not the next READY, and refuses
  routed input until a new owned boot. Its own record check is defence in depth:
  the negotiated stream's check already fails it, and only the contract pins it.
- A limit record read while the coherence gate waits for its WINLIST reply now
  ends it with "guest restarted during inventory" rather than at the deadline
  120 s after `BVAGENT SERVICE start`. The gate's earlier phases have no reset
  check, so a reset there still ends in a TimeoutError.
- Lines that only start with `PSCI SYSTEM_RESET:` no longer cancel anything.

## Evidence and limits

On the unmodified detectors 7 BridgeVMControl shim tests, 5 of the 6 contract
tests, the coherence observation test and the window-inventory restart check
failed. With the change the four shim suites passed under ThreadSanitizer, as
did SwiftPM XCTest on the affected suites, the contracts and standalone builds.

Open: guest serial output reaches run.log, so a guest can print an exact record,
which only cancels. The session still reports a connection through the reboot
window. No end-to-end evidence exists: no multi-generation app run.log is retained.
