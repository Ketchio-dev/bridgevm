# Apple silicon live gates

Document status: **Current**
Last reviewed: **2026-11-23**

Live gates are the only evidence that can promote a Windows HVF capability.
They need bare-metal Hypervisor.framework, WindowServer/CGL, private Windows
media and a real GPU, so they cannot run on hosted CI. This page defines where
they run, how they are scheduled, and what may leave the machine.

## Where work runs

| Work | Where | Why |
| --- | --- | --- |
| fmt, clippy, unit/integration tests, MSRV, docs and capability drift, dependency policy, fuzz smoke, host Venus build | GitHub-hosted Actions in the public repository | Deterministic, no private media, no virtualization |
| Bare-metal HVF boots, snapshot restore-boot, packaged install and import journeys, audio teardown | The local physical Apple-silicon development Mac, through a local job queue | Requires Hypervisor.framework, a real GPU and private Windows media |

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
JOB_ID="$(scripts/live-gates/bridgevm-live submit t1-snapshot)"
scripts/live-gates/bridgevm-live status "$JOB_ID"
scripts/live-gates/bridgevm-live logs "$JOB_ID"
scripts/live-gates/bridgevm-live receipt "$JOB_ID"
scripts/live-gates/bridgevm-live cancel "$JOB_ID"

# Sealed-input tiers take an operator-created local TSV whose rows are key,
# absolute path, SHA-256. The submit command copies it into the atomic job
# directory.
scripts/live-gates/bridgevm-live submit t17-windows-hvf-product-e2e \
  --input-manifest ~/BridgeVM/t17-inputs.tsv
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

`bridgevm-live submit` accepts exactly these tiers. Every tier except
`t1-snapshot` needs `--input-manifest`. `t0-check` is refused: deterministic
checks belong on GitHub-hosted Actions.

| Tier | Scope | Purpose |
| --- | --- | --- |
| `t1-snapshot` | Powered-off snapshot pair gate on the canonical disk and vars ([`verify-powered-off-snapshot.sh`](../../scripts/verify-powered-off-snapshot.sh)) | A19: byte-exact create/restore of the pair; a torn pair is refused |
| `t1-restore-boot` | Sealed release snapshot, restore and Windows boot ([`run-snapshot-restore-tier.py`](../../scripts/live-gates/run-snapshot-restore-tier.py)) | A19: a restored snapshot boots and keeps the guest marker |
| `t17-windows-hvf-product-e2e` | Packaged Windows-HVF 3D-off product E2E from an ISO | One-lane diagnostic pilot or fixed three-lane release campaign |
| `t18-audio-teardown` | Fixed CoreAudio playback and shutdown | B7 typed callback teardown evidence; exactly ten isolated lanes |
| `t19-windows-hvf-import-product-e2e` | Packaged installed-disk import product E2E | One-lane diagnostic pilot or fixed three-lane release campaign |
| `d5-guest-input` | Guest input on a sealed installed pair | Development diagnostic; never promotes a criterion |
| `d6-app-ui`, `d6-app-ui-host-v1`, `d6-app-ui-host-v2`, `d6-app-ui-install-cancel-v1` | One sealed app-only native window run, never a guest workload | Development diagnostics; never promote a criterion |

Use `d6-app-ui-host-v2` for the rendered onboarding journey. Its signed external
accessibility driver opens Create and Import, filters the seeded VM list and clears
the search in the actual app window. The older `d6-app-ui` XCTest and
`d6-app-ui-host-v1` read SwiftUI accessibility children in process. On macOS 27.0.1
that read can return an empty tree even when the controls respond to the external driver.

No faster tier may be used to lower a threshold, and a `d` tier receipt is
never release evidence.

T17 never substitutes an installed disk, harness acknowledgement or synthetic
guest log for the packaged product command. Its private manifest fixes one
packaged app, product helper, runner, firmware, Secure Boot policy, vars seed,
guest payload, external payload manifest and user-supplied ISO by SHA-256.
Missing app, ISO or guest-payload inputs produce a public `preflight-blocked`
receipt with zero runs. Each sequential lane has a separate product library,
share, VM slug, disk, vars, vTPM and powered-off snapshot. The requested disk,
vars, vTPM and snapshot paths are the product-created files below
`<library>/<slug>/bundle.vmbridge`; copying them to harness-owned evidence
paths does not satisfy the contract. The host verifies the snapshot directory's
manifest plus disk/vars pair and the actual lane-local installer-source cache
receipt before cleanup. A pilot is one diagnostic run. Release is
fixed at three of three, but even a passing receipt keeps `criterion_pass` and
`capability_promotion` false: the 3D-off gate cannot close A9.
Submission also refuses unless this user's console session is on screen and
unlocked; a lock after submission is not detected.
The TSV contains exactly `campaign_mode` (`pilot` or `release`) plus
`app_bundle`, `app_executable`, `product_helper`, `runner`, `firmware`,
`secure_boot_policy`, `iso`, `bundled_vars_seed`, `guest_payload` and
`guest_payload_manifest`. The guest payload is a symlink-free tree and its
manifest must be outside that tree. The helper must be the sealed app-local
`Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E`
command, whose fixed bundle identifier is `dev.bridgevm.product-e2e`, and accepts only
`--windows-product-e2e --request FILE --result FILE`; an artifact without that
command is a preflight blocker, not an invitation to invoke a harness. Private
paths, ISO, media, vars, vTPM, guest payload and helper logs never enter the
public receipt; the dedicated verifier is run both before and after redaction.
A first-READY failure keeps a private diagnostic packet, and so does an
authenticated lane that reached first READY and then failed
`guest-evidence-missing` (`packet_kind` `post-ready`), which adds a private
share listing of names, types, sizes and SHA-256 only. A post-READY packet
keeps the run-log tail, that listing and the guest-setup harvest. The 3D-off
runtime disables virtio-gpu, and the r35 packet recorded `display.fb` absent
because only the virtio-gpu device wrote it then. The probe now also publishes
ramfb frames to `display.fb` when the machine has no virtio-gpu device, so a
later 3D-off packet's existing `display_fb` row can hold the last frame the
probe exported; no live run has shown this yet, and that frame is not tied to
a host stop. The helper takes no nonce-bound host stop after READY, so no
final ramfb pair is attributed. Keeping an attributed failure screen needs the
helper to take that host diagnostic stop before its graceful stop. Each
packet's guest-setup
harvest, whose allowlist includes `C:\bvagent.log`, clones the lane disk,
attaches the clone read-only and mounts the Windows NTFS volume read-only below
`<lane>/guest-setup-harvest`. The harvest's
120-second deadline is checked only between tool calls, allowlist items and
1 MiB chunks, so it does not bound a blocked open or read on the guest volume.
The tier reports `cleanup-failed` and keeps its job tree when `mount` or
`hdiutil info` cannot be read within 20 seconds or still lists the tree, or
when the harvest did not prove its release. A queue cancel sends SIGTERM to the
tier's process group. The tier then ends every process whose command line names
its lane tree (the helper runs under LaunchServices, so it and the app, runner
and swtpm it starts are outside that group) and runs this cleanup; T17 and T19
get 2 minutes before the worker's SIGKILL, which can still stop the harvest
before it detaches. The job then has no tier receipt and the worker
fences the queue. Before clearing that fence, look in `hdiutil info` and
`mount` for an attached `disk.raw` clone and a mounted guest volume under
`/private/tmp/bridgevm-e2e-<job>.*/lane-N/guest-setup-harvest`, and unmount
and detach them before removing the tree.
Before removing the tree, cleanup forgets each lane's vTPM key through the
sealed app's own `--vtpm-lifecycle export` and `forget-import`
(`scripts/live-gates/t17_lane_key_forget.py`). It keeps the tree and reports
`cleanup-failed` when the app executable no longer matches its seal, when a
lane's request, slug, vTPM path or library departs from the lane's fixed
layout, or when the product neither prints `forgot_imported_key=<slug>` nor,
for a lane whose vTPM state is empty or absent, reports the key missing. Seal
and layout refusals run no app; after a product refusal the other lanes are
still forgotten, and each refused lane is named. The recovery package and code
are removed whatever the outcome: SIGTERM, SIGINT or SIGHUP, such as a queue
cancel's SIGTERM, ends this step with status 128 plus the signal number only
after that removal. Only SIGKILL can still leave them, for example the cancel's
SIGKILL 2 minutes later if it lands while the cancel's own cleanup runs
this step again. Earlier T17 receipts reported
`worker_cleanup_verified` without forgetting these keys.
T19 seals a packaged app, fixed-identity helper, runner, installed disk, exact 64 MiB vars and symlink-free vTPM tree; each lane imports independent read-only clones through `--windows-import-product-e2e`, runs the full guest journey, reauthenticates canonical inputs, and retains `claim_eligible=false` because A9 also requires T17.

T17 and T19 turn off Mac clipboard sync in the lane VM settings and check the
saved setting before starting the installed VM. The clipboard stage checks a
synthetic Windows clipboard round trip through the guest agent.

## Foreground wait policy

Local deterministic developer checks, including `scripts/check-project.sh`,
may run in the foreground for up to 300 seconds. This local result does not
replace the required GitHub-hosted CI result for a pushed SHA. If a deterministic
check is expected to exceed 300 seconds, run it on GitHub-hosted Actions and
record the run id.

The physical-Mac live queue is reserved for work that requires a physical
Apple-silicon Mac, bare-metal Hypervisor.framework, WindowServer/CGL, private
Windows media or a real GPU. Submit that live work asynchronously and record
its job id. Slow deterministic checks do not belong in this queue.

These follow the CI and live-run rules in [AGENTS.md](../../AGENTS.md) (sections 6 and 7).
`sleep 6000`, long `while ps; sleep` loops and multi-hour synchronous tool calls
remain prohibited.

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
