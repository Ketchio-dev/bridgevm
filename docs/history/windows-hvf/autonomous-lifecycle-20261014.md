# Display, import and owned controls — 2026-10-14

Evidence rank: **test**. This fourth isolated integration follows
[the prior controls checkpoint](autonomous-controls-20261014.md) and
[owned restore recovery](autonomous-restore-ownership-20261014.md).
Focused counts overlap existing suites; combined exact-source local and hosted
checks remain separately required.
No live gate, guest behavior or release criterion is proved by these results.

## CLI installation guidance

Public `app create-windows` and `app install` help omitted the existing native
warning about registrations lacking the signed ARM64 guest driver payload.
Both commands now share the existing guidance: installation is blocked for
those entries; create a new VM in the app with ISO, signed payload and manifest.
Existing CLI entries cannot be completed through the app or CLI.
Source conclusion: `558de603199df53e1bc2f4ee21391959a63b7e76`.
An isolated parse/help executable compiled the exact production clap argument
modules against the workspace clap library. All 28 help cases (14 commands,
both help flags), parent help, three parser controls and normalized native
wording passed; formatting and the existing 20-link landing contract passed.
The baseline lacked the warning. A first preservation assertion differed only
in trailing newline; its comparison was corrected without a product change.
No command handler, installed full CLI binary, app or guest was executed.

## Framebuffer loss revokes stale input

Removing the backing file left `FBLayerView` input available; subsequent actual
view insertion appended TEXTINPUT. The two-method baseline failed two
assertions in one method; the unchanged-frame control passed. A separate
one-method baseline failed two assertions because movement and left drag
received while unavailable replayed POINTERINPUT when a frame recovered.

Backing removal, replacement or invalid newly published geometry now clears
the stale image/geometry, cancels marked text and deferred motion, and attempts
existing held-pointer cleanup once. Unavailable motion is rejected at event
admission. Validated recovery accepts fresh input; unchanged/odd sequences,
provider retry and a valid cached IOSurface retain their existing behavior.
Source conclusion: `19d979c58da567b3ad89ad5e05fe6aa86c6a9334`.
Final native 51 and actual-shim 51 cases passed, with zero skips in the shim.
An intermediate eight-method run failed three assertions in one fixture that
changed width without publishing a new sequence. Publishing sequence 4 fixed
that fixture; production did not change for this correction. These are
synthetic AppKit callbacks and tiny framebuffer tests. Cleanup append does not
prove guest consumption or button release. No WindowServer rendering is
claimed, and existing validation does not eliminate every truncation race.

## First-run symbolic-link sizes

Metadata size checks measured a symbolic link instead of its selected target.
A valid 64 MiB vars target was refused; an empty disk target was accepted and
actually registered with a zero-byte copy. Two separate native baselines each
ran two methods and failed two assertions; direct-input controls passed.
The two size expressions now resolve the target while preserving original-path
errors, the nonempty-disk and 64 MiB requirements, and existing copy/publication.
Source conclusion: `24eee675dcf03c0ba65589be61ac7c89ad339821`.
Final native 16 and actual-shim 16 cases passed with zero shim skips: eight alias
admission, three disk-format and five publication-transaction cases. Actual
worker imports verified registration readback, independent copies and unchanged
source hashes/inodes; wrong-size, dangling, directory and empty targets refused.
Fixtures used tiny synthetic disks and sparse vars. This covers symbolic links,
not Finder bookmark aliases or concurrent retargeting; no real keys or guest
media were used; no boot or clean-machine installation is proved.

## Generic retained controls and process ownership

An active FastVZ/QEMU model was released after missing or malformed registration
was reloaded through ordinary library add/selection. Two baseline methods
failed eight assertions. This was previously excluded scope, not a newly
introduced regression. Retaining the old view alone could not make path-based
Stop safe, so fresh launches now return exact in-memory Process-group handles.
Source conclusion: `107794345f7992c71a7cd7fede627e1322c4774f`.
Owned Stop signals only captured children, serializes pending cancellation with
launch, and waits for the main child before stopping a newly owned TPM. Partial
failure and timeout keep cleanup activity reachable; the monotonic 12-second
acknowledgment bound adds no SIGKILL escalation. Reused TPM remains explicitly
unowned and is never signaled by owned cleanup. Opted-in refusal never falls
back to a second legacy launch, and terminal ownership never adopts a later
path-matched process. No deinitialization or app-quit termination was added.

The current sidebar retains the original model/config identity, status, owned
Stop and terminal-only dismissal. Accepted resource changes can finish on that
model; new retained start/resource/terminal/package actions refuse at model
entry. Shared/nested asset guards protect conflicting work, deletion/move and
external shared-disk clone. Ordinary registered unowned Stop compatibility
remains; retained unowned Stop is disabled. Ownership is not restored after
app relaunch, and ordinary registered guest-command admission is unchanged.

Final native 108 and the same actual-shim 108 cases passed with zero shim skips,
including 28 new methods. A standalone parent built from unchanged production
launch/runtime sources exited normally; its child was observed alive under
PID 1 before exiting naturally, without a child signal. This is process-level
survival proof, not GUI app-quit, VM or guest proof. Existing regular log
truncation/output/permissions passed; new logs intentionally use 0600, and
nonblocking descriptor admission refuses symlink and special-file outputs.

Retained failures include a test-only `waitUntilExit` cleanup stall, repaired
with bounded polling; a 97-method run with seven assertions (one unexpected)
regressed existing metadata-deferral behavior, repaired without changing
that test; and a one-method/two-assertion shared-disk clone baseline. A later
108-method run failed four assertions because its new resource fixture acted
before startup confirmation. Awaiting ordinary confirmation repaired the
fixture while retaining all removal/restart/Stop/dismiss assertions.

## PCI ISR acknowledgment

Net, console and GPU PCI ISR reads returned status without clearing it.
Three exact-offset reads now use `mem::take`, satisfying read-to-clear in
[OASIS VIRTIO 1.2 §4.1.4.5.1](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html).
The exact unchanged six-test suite changed from one pass/five failures to
six passes; related virtio tests passed 245/0, with Clippy and formatting clean.
Earlier four- and five-case failing baselines remain recorded. Actual synthetic
RX completions, access widths, adjacent offsets, reset/rearm and masked MSI-X
pending-bit controls are covered. MMIO explicit ACK and MSI-X PBA semantics
are unchanged. No guest, physical IRQ or INTx routing proof is claimed.
Source conclusion: `7800849e11c711982c92ceea4c3c7cd288cc9cb9`.

## Evidence boundaries

Independent reviews and integration receipts bind the five source conclusions
to their focused evidence. Structural budgets passed: new modules use actual
sizes and no existing ceiling was raised. Failed runs remain in the record.
No private paths or guest assets are included; capability wording and gates are unchanged.
A9, A11, A19 and product-state promotion remain governed by the capability
manifest and their existing live criteria; none is closed by this checkpoint.
