# HDA RUN resume preserves status and DMA position — 2026-10-05

Integrated source checkpoint
`245daf7744ed919feb4190c3d1f20320741d217c` repairs two RUN restart defects.
Public MMIO and DMA reproduce the original behavior on
`15f97bce666c433742844e591c2965ad4d7429b5`: restarting playback clears an
unacknowledged completion status and replays data from the first descriptor.

## Reproduced transitions and controls

Five cases use two valid 192-byte descriptors, CBL384 and LVI1. Completion
status is generated through DMA, then survives stopping RUN but disappears
when RUN restarts. Paired controls separately acknowledge BCIS through guest
write-one-to-clear or clear it through stream/controller reset.

Separate cursor cases stop halfway through descriptor0 or at descriptor1.
After resume, they inspect LPIB288 and the exact PCM sequence delivered to an
in-memory sink. Polling while stopped changes neither output nor position.
Neither case wraps the cyclic buffer, and neither enables IOC, isolating
position from the completion-status case.

The unchanged production baseline is 23 PASS / 3 FAIL: completion retention
and both cursor cases fail; the W1C/reset controls pass. Baseline raw SHA-256:
`88a8b57fa0a355c594450b0ee07a137e4bacf864ece1b90f776758260eb41f48`.

## Narrow repair and paired results

`crates/bridgevm-hvf/src/hda/stream_control.rs` no longer resets stream status
or the DMA cursor when RUN changes from 0 to 1. The obsolete reset helper is
removed from `crates/bridgevm-hvf/src/hda/hdapcmsink.rs`. Guest W1C, explicit
resets and restart timing behavior are unchanged.

All five regression fixture bodies are byte-identical between the baseline
and repair. The complete HDA test filter passes 26 tests in debug and 26 in
release. Debug raw SHA-256:
`3f7dd2ab5bfd1436f76c17bd869bf2b264c03534d0b27122b178265b41626bdc`;
release raw SHA-256:
`b68c163f3f0330c7475cac6672b87dc96e82bd73dfa59ef8e7474e44b8346f4e`.
Focused library Clippy, formatting, structural budgets and whitespace checks
pass. The initial formatting check failed only module ordering; that failure
is preserved, then corrected without changing either fixture body.
Independent review authenticates the paired receipts and exact production
inverse without replaying tests. The core ceiling decreases from 761 to 752;
the two new test files are registered at their actual counted sizes, 107 and 73.

Intel HDA 1.0a section 3.3.36 defines BCIS as write-one-to-clear, while section
4.5.5 requires DMA resume at its prior position. These expected semantics are
restored without a new intentional machine-contract deviation. See the
[primary Intel specification](https://www.intel.com/content/dam/www/public/us/en/documents/product-specifications/high-definition-audio-specification.pdf).

## Evidence boundary

These are deterministic device-model results, not Windows audio-quality or
hardware criterion evidence. The dated 15f T12 fixed 20/20 physical NVMe result
predates the new HDA/GIC source and does not test either repair.

This combined successor still requires complete local project and exact-SHA
GitHub-hosted checks. All 29 criterion states, thresholds, known defects and
product wording remain unchanged. No VM, Windows audio improvement, guest
performance gain or criterion/release promotion is claimed for this repair.
