# Import and admission follow-up — 2026-10-14

Evidence rank: **test**. This fifth isolated integration follows
[the lifecycle checkpoint](autonomous-lifecycle-20261014.md). Focused counts
include overlapping existing suites and must not be summed as unique tests.
Combined exact-source local and hosted checks remain separately required.
The earlier hosted failure remains failed; wave3 and wave4 both contain the
affected assertion. This repair does not make those earlier sources release
eligible or promote any live criterion or product state.

## Hosted warning-path fixture

Hosted PR CI `38517590332` failed two warning-navigation methods on each of
macOS 15 and 26. The scanner returned canonical issue paths while the tests
expected paths containing an aliased ancestor. The initial local fixture
symlinked the library root itself: three methods failed eight assertions,
and its attempted correction also failed. That fixture exercised existing
root-symlink refusal, not the hosted ancestor-path mismatch; both runs remain.

A regular library under an owned aliased ancestor reproduced the actual
mismatch: three methods, two assertion failures, with the healthy control
passing. Expected issue paths now resolve their ancestors. All original
route/content/VM-count assertions remain. Final native 11 and actual
thread-sanitized-shim 11 cases passed with zero shim skips. Product scanner
behavior and direct-root refusal are unchanged; this is a test portability
repair, not a hosted pass for the earlier failing SHA.
Patch SHA-256:
`858b89b52afdb775a362e2925ef7719dc7a85b3b208b26e4c9b997e5955e4bf9`.

## Literal bundle filenames

On Unix, copy summaries replaced every literal backslash with a separator.
Directory and tar exports succeeded but importing a raw primary with such a
name failed ENOENT. The corrected baseline had two passing plain controls
and two product failures; source and export bytes remained unchanged. Its
first attempt also had a fixture-name collision (one pass, three failures).
Only copy-summary emitters now normalize the native `MAIN_SEPARATOR`.
Traversal, symlink and member-type admission are unchanged.

Eight public storage-API cases passed, covering plain/literal primaries, extra
literal filenames and collision with actual nested names, each as directory
and tar. Existing import 23 and bundle 11 selections passed; these overlap.
Clippy and formatting passed. A first formatting failure remains recorded.
An early scratch-only budget table incorrectly raised existing 80/32 ceilings
to 81/33; its apparent pass is explicitly invalid. Final source retains 80/32,
registers the new test at 170 actual lines and passes the no-increase check.
Proof covers Unix UTF-8 names, not arbitrary-byte or Windows runtime behavior.
Patch SHA-256:
`c0eac00dad599c4130c3e9b8d288c4867eb5f9c404b7b1af497e2d8cf11218a2`.

## Block-device PCI ISR acknowledgment

The separate read-only boot-media virtio-blk model completed synthetic reads
but did not clear ISR on legacy PIO or exact BAR4 ISR reads. The final unchanged
seven-case baseline passed four and failed three; the earlier five-case
baseline passed two and failed three. The repair returns the prior status and
clears only the exact PCI ISR accesses, following
[OASIS VIRTIO 1.2 §4.1.4.5.1](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html).

All seven cases and 32 related library cases passed; Clippy and formatting
passed. Tests check completed synthetic requests, sector/status/used-ring
contents, repeat acknowledgment and recorded platform SPI35 deassertion.
Core virtio-MMIO InterruptStatus and BAR common alias 0x60 remain observations;
explicit ACK 0x64 still clears matching bits. Compatibility writes, adjacent
reads and reset controls remain. MSI-X proof is table/PBA noninterference,
not block message delivery. The extracted register map is byte-identical.
No guest boot, physical IRQ, disk durability or complete modern common-config
conformance is claimed; writable NVMe and managed snapshot state are separate.
Patch SHA-256:
`6e8f80958790c5f4b6c1f795f11e3fde4178e4fb901d37e02bf88a2a067b27e5`.

## Registered generic writer and clone admission

The prior generic repair protected retained controls. A still-registered,
already cached active FastVZ model could share an idle HVF VM's disk while
`cloneWindowsHVF` admitted that disk's clone. The actual-route baseline ran
two methods and failed three assertions; distinct disks remained admitted.
The scheduler captured the job without executing copy or Keychain work.

Clone admission now checks existing active non-HVF cached models with the
same symmetric asset-overlap logic. Activity includes accepted work and
owned cleanup after primary exit. Cache queries/binding were coherently
extracted; the private cache remains private. No discovery, materialization,
process scan or media-lease redesign was added. Retained work, deletion/move
and ordinary unowned Stop policies are unchanged.

The byte-identical baseline now passes 2/0. Expanded native 113 and the same
actual-shim 113 cases pass with zero skips: alias/inactivity controls, actual
pending resource work and a bounded synthetic owned-TPM cleanup all reach the
clone route. QEMU alias coverage is model admission, not a QEMU raw launch.
No corrupt or completed clone was observed. A generic start after a clone
has already been admitted is a separate reversed-order case, unproved and
unfixed here; this is not general cross-process writer exclusion.
Patch SHA-256:
`bf3a8247de87e6872e5df909b3ec452baa977cda82250b1a46553e99ffeaa55c`.

## Clearing optional vTPM state in first-run import

Selecting optional state and recovery drafts, then clearing state, hid the
recovery fields but still submitted their paths. Actual default-action
submission failed the strict recovery-without-state check: one baseline
method, one assertion failure. The view now omits inactive recovery inputs
while preserving drafts for reselection. Global validation remains strict.

Final native 4 and actual-shim 9 cases passed with zero shim skips; the latter
includes five existing transaction controls. Installed SwiftUI bindings were
updated through native text-edit delegates in a deferred window never shown
or focused. Synthetic default-action submission reached the actual worker,
real helper and registration readback after copying tiny disk/sparse vars.
Reselection restores drafts; missing selected recovery still refuses; explicit
recovery-without-state still fails global validation. An earlier inspection
failed two assertions because it read uninstalled State from a reflected
rootView copy; that fixture failure is retained, not a product baseline.
No actual vTPM recovery files, key operation, guest or WindowServer input is
proved. The successful paths omit state and return before key operations.
Patch SHA-256:
`ca4be25080303a9c332e83f7c251b911b5461715224409a5b9ca31acb0aab5dc`.

## Boundaries

These are focused deterministic source conclusions, with retained failed
experiments and hash-bound receipts. Existing structural ceilings are not
raised; new modules use actual sizes. No private paths or guest assets are
included. Capability wording, live sample counts and gate definitions remain
unchanged. Exact integrated project and hosted checks must still be recorded;
none of these tests substitutes for A9, A11, A19 or live release evidence.
