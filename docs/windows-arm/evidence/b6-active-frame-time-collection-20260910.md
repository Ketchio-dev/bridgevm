# B6 active-scene collection contract

Date: 2026-09-10. Code: `1d03dea5`. B6 remains OPEN.

## Collection gap

The cell harness previously finished typing, captured the scene, disabled the
classic caret, and then synchronously collected PresentMon for 15 seconds.
It sent no scene-changing input during that collection. The retained classic
run2 CSV authenticated in the preceding investigation contains only three
frames; it is not a representative workload or a performance baseline.
The packaged scene had no FrameTime collection at all.

## Implemented path

- Both classic and packaged scenes now collect through the existing guest
  PowerShell collector while the host sends a text input every half second.
  The collector waiter alone uses the agent command channel; scene input
  uses the separate HID control channel.
- The helper verifies the expected foreground HWND before and after the
  collection, requires input acknowledgments, and bounds its collector wait.
  These endpoint checks do not prove uninterrupted foreground ownership.
- An existing host CSV is refused, including a dangling symlink. Only a
  matching `BVPRESENTMON` filename/hash report in the newly appended guest-log
  interval supplies the expected CSV SHA256.
- The consumer waits for that exact byte identity, rather than accepting the
  first partial share transfer. It then uses the contiguous FrameTime parser,
  which authenticates the file again. Conflicting guest hashes fail closed.
- A cell observation cannot report complete when either scene lacks a valid
  authenticated collection. Reports retain false claim, criterion, and
  capability-promotion flags.

The input-command count is not a count of rendered glyphs. These diagnostics
do not establish baseline provenance, adequate performance sample coverage,
the 27-run matrix, reviewed caption/menu and tab/menu masks, or the original
within-10-percent baseline clause. Input latency is not substituted for
FrameTime. No performance threshold or product wording was relaxed.

## Deterministic evidence and remaining live work

`tests/integration/b6-active-frame-time-contract.py`: 14 headless tests passed
locally in 1.686 seconds. Fixtures cover fresh versus stale reports, conflicting
identities, filename/prefix separation, partial transfers, timeouts, symlinks,
invalid CSV, the host input loop, quoted paths, failed commands, focus refusal,
existing-output preservation, and missing input acknowledgment.

The fixtures simulate the guest and agent functions. They do not run Windows,
ETW, CGL, or a VM, and do not prove the live integration. A new GitHub-hosted
workflow runs this deterministic contract; real GPU/media work still belongs
on the physical-Mac queue.

Preceding checkpoint `c5d3866fbb37c6422502ae749de31a6b98b93b74` has green
[CI 34435232868](https://github.com/Ketchio-dev/bridgevm/actions/runs/34435232868),
[Security 34435232865](https://github.com/Ketchio-dev/bridgevm/actions/runs/34435232865),
[collector contracts 34435232984](https://github.com/Ketchio-dev/bridgevm/actions/runs/34435232984),
and [FrameTime contracts 34435232896](https://github.com/Ketchio-dev/bridgevm/actions/runs/34435232896).
Those results do not validate the later active-collection change. Its full
project check and exact-checkpoint hosted results must be recorded separately.

No live B6 job or product-state promotion is claimed by this document.
