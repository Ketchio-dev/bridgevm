# Apple silicon live gates

Document status: **Current**
Last reviewed: **2026-08-30**

Live gates are the only evidence that can promote a Windows HVF capability.
They need bare-metal Hypervisor.framework, WindowServer/CGL, private Windows
media and a real GPU, so they cannot run on hosted CI. This page defines where
they run, how they are scheduled, and what may leave the machine.

## Where work runs

| Work | Where | Why |
| --- | --- | --- |
| fmt, clippy, unit/integration tests, MSRV, budgets, docs and capability drift, dependency policy, fuzz smoke, host Venus build | GitHub-hosted Actions in the public repository | Deterministic, no private media, no virtualization |
| Bare-metal HVF boots, guest install, GPU/title measurement, reset soaks | The local physical Apple-silicon development Mac, through a local job queue | Requires Hypervisor.framework, a real GPU and private Windows media |

Hosted CI stays authoritative for everything it can run. Moving an ordinary
check to the local queue to hide a hosted failure is forbidden.

## Why there is no self-hosted GitHub runner

The public repository has `pull_request` workflows, and GitHub's own guidance is
not to attach self-hosted runners to public repositories: a workflow reachable
from a fork could execute untrusted code with the runner account's privileges,
next to the VM images, Keychain and SSH keys on a personal machine.

The agent already executes commands directly on the development Mac, so a runner would
add that risk without making a single test faster. What was actually missing is
**asynchrony**, not a runner. The local queue provides it.

The queue has no Mac Studio model dependency. Some script and service names
retain `studio` for compatibility with the original machine, but the active
worker may be this MacBook Pro or another explicitly installed local Mac.

A dedicated secondary node (for example a second Apple silicon Mac used only for
overnight runs) is a possible later addition. It is out of scope here, and no
criterion in the capability registry depends on a second host.

## The local queue

```sh
JOB_ID="$(scripts/live-gates/bridgevm-live submit t3-candidate)"
scripts/live-gates/bridgevm-live status "$JOB_ID"
scripts/live-gates/bridgevm-live logs "$JOB_ID"
scripts/live-gates/bridgevm-live receipt "$JOB_ID"
scripts/live-gates/bridgevm-live cancel "$JOB_ID"

# A3 uses an operator-created local TSV whose rows are key, absolute path,
# SHA-256. The submit command copies it into the atomic job directory.
scripts/live-gates/bridgevm-live submit t6-a3-title \
  --input-manifest ~/BridgeVM/a3-inputs.tsv
```

Properties the queue must hold:

- `submit` returns a job id in under 10 seconds and never blocks on the run.
- The job runs from a detached worktree pinned to an exact commit, with its own
  `CARGO_TARGET_DIR`, so later edits to the development checkout cannot change
  what a submitted job measured.
- The commit, binary, image, vars and driver hashes are sealed into the job
  manifest at submit time and repeated in the receipt.
- A resource lock serialises jobs that need the GPU or the canonical images.
- The worker outlives the agent session; results are read later by job id.
- There is no inbound listener and no network service.

## Test tiers

| Tier | Scope | Purpose |
| --- | --- | --- |
| T0 | Deterministic project check | Rejects broken changes in minutes |
| T1 | 60-second bare-metal vtimer/PSCI microprobe | Tests the timer/cancellation contract without booting Windows |
| T2 | One prepared-cache pilot boot | Confirms the image and injector pipeline |
| T3 | 3-boot candidate gate | Filters functional changes before a full campaign |
| T4 | Nightly 100-reset soak | Reset lifecycle evidence |
| T5 | Weekly/release 10-boot A1 gate | A1 shipping evidence |
| T6 | Three independent PPSSPP + DXVK real-title runs | A3 shipping evidence |
| T7 | Fixed Windows closure campaign | Combined installed-guest acceptance evidence |
| T8 | Fixed 20-lane pointer campaign | B4 pointer reliability evidence |
| T9 | Fixed 20-lane BridgeVM PC firmware campaign | Experimental-board standard UEFI PCI development evidence |
| T10 | Sixty loaded full-workspace rounds | QMP shutdown-race regression evidence |
| T15 | One sealed 4-vCPU release boot | Interleavable diagnostic sample for A/A noise and A/B performance comparisons |

T0–T4 filter candidates. Only T5 produces A1 shipping evidence, and no faster
tier may be used to lower a threshold. T6 requires all three runs to report
guest samples, p50 at or above 30 FPS, and exact title-local DXVK module hashes.
Its submit command copies and hashes a local input manifest; the worker verifies
every listed image, vars, title, binary, driver tree and renderer hash before it
builds or boots anything. The TSV has exactly three fields (key, absolute path,
SHA-256) and exactly these keys: `image`, `vars`, `title`, `ppsspp`, `d3d11`,
`dxgi`, `viogpu_dir`, `virglrenderer`, `moltenvk`, and `binary`. `viogpu_dir`
uses the stable digest of its sorted relative file names and content hashes;
every other row hashes the named file bytes. The `ppsspp` row is a sealed ZIP
payload whose only executable identity is
`ppsspp/PPSSPPWindowsARM64.exe`; validation rejects absolute/traversal paths,
symlinks, Windows path aliases/reserved names, duplicate case-insensitive
entries, unsupported/encrypted compression, more than 4,096 entries, and more
than 256 MiB compressed or expanded content. Each run stages that exact archive in files no larger
than 7 MiB, reconstructs it in a separate guest staging directory, verifies
both archive and executable SHA-256, and only then removes the previous title
tree and moves the verified staging tree into place. This replacement is not
claimed crash-atomic; a failed replacement fails before title launch, and the
next run reconstructs from sealed bytes rather than trusting the partial or
previous tree. Submit copies the signed release `binary` into the
job directory, and the worker runs those exact sealed bytes rather than
rebuilding after submission. Receipts preserve separate PPSSPP payload and
embedded-executable hashes.

T9 is deliberately not assigned a product capability criterion. It rebuilds
the experimental BridgeVM Virtual ARM PC firmware and HVF runner from the
sealed commit, then requires 20 independent lanes. Every lane owns a fresh
variable-store file and must pass both a write process and a separate restore
process, for 40 successful firmware boots. Passing T9 proves only the bounded
standard UEFI PCI enumeration contract named in its receipt; it does not prove
BAR operation, DMA, interrupts, Block I/O, GOP, BDS or Windows boot.

## Foreground wait policy

An interactive turn must not wait on a long gate. Commands expected to exceed
120 seconds are submitted to the queue, and the turn continues or ends with the
job id recorded. `sleep 6000`, long `while ps; sleep` loops and multi-hour
synchronous tool calls are prohibited.

Hosted CI is polled for at most 180 seconds; after that the run id is recorded
and read on a later turn.

## Retention and what may be published

Runs are kept under `~/BridgeVM/runs/`. Guest disks, UEFI vars, vTPM state,
recovery keys and third-party title content are local-only and never leave the
machine.

A receipt is publishable only after redaction to the allowlisted fields in
`schemas/bridgevm-capability-v1.json` (`gate_receipt`): gate id, criterion,
tested commit, host OS/hardware, input hashes, sample counts, pass/fail counts,
evidence paths and known confounders. Hashes and counts may be published;
paths into private media and any guest secret may not.

A free-space guard stops a nightly rather than deleting canonical inputs.
Canonical images stay on external storage; the internal disk holds only the
prepared cache, worktrees and logs.
