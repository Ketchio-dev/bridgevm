# A19 ten-lane product lifecycle campaign (T23)

Classification: reference for running the physical-Mac tier. The A19 statement,
its state and its known defect are owned by `capabilities/windows-hvf.json`;
this note changes none of them. A19 names "a fixed ten-lane product lifecycle
campaign passes 10/10 at the release head". Tier
`t23-a19-lifecycle-campaign` is that campaign. The single-sample T20 tier
(`t20-a19-native-snapshot-restore`) and its receipts are unchanged.

## What the tier runs

One queued job runs exactly ten lanes one after another, all at the job's
sealed commit, sealed input manifest and sealed probe
(`scripts/live-gates/run-a19-lifecycle-campaign-tier.py`). There is no pooling
across jobs or sources. Each lane runs these steps:

1. It reauthenticates the manifest, then APFS-clones the release app, the
   canonical installed disk and its matching vars into its own
   `lanes/lane-NN/prepared-inputs`. It refuses media that is linked, is a
   symlink or shares an inode with another lane file or the canonical pair.
2. It runs the unchanged T20 lifecycle,
   `scripts/verify-native-snapshot-restore-boots.sh`, under its own VM ID
   (`a19-campaign-lane-NN`) and its own native library. The lifecycle writes
   the original marker, creates a packaged-CLI snapshot and writes a clobber
   marker. It then restores, exports the restored pair to a fresh path and
   boots that export. Each of the three boots must end in a host-framed
   natural shutdown.
3. It removes its clones and verifies that neither `prepared-inputs` nor
   `live` remains before the next lane may start. It retains a path-free
   `lane-result.json` that binds the job, commit, manifest and probe hashes.

The first failed lane ends the campaign, as it does in B7 (T18). A lane is
never retried and never replaced. A lane whose cleanup cannot be proven ends
the campaign as `cleanup-failed` and fences the queue.

## What a pass means

`pass=true` requires all of the following:

- the ten lanes ran in order and each lane passed;
- 30 of 30 boots passed, each ending in a natural shutdown;
- every lane's original marker came back and its clobber marker differed;
- the markers are distinct across lanes;
- every sealed hash and lane hash is present; and
- every lane's cleanup is verified.

A run of 9/10, a missing lane, a duplicated or reordered lane, or a failed lane
followed by more lanes cannot verify as a pass.

`claim_eligible`, `criterion_pass`, `capability_promotion` and
`three_d_injection` are always false. The receipt cannot establish the release
head, green hosted CI for the SHA or Developer ID signing. It also cannot
establish A19's quota and atomic-pair parts. Physical power loss is outside
A19. A passing receipt is the campaign evidence only. Closing A19 remains an
operator decision recorded in the capability registry.

The receipt holds only hashes, counts and flags. It carries the sealed input
hashes and aggregate counts (`run_count`, `passes`, `failures`, and boot and
natural-shutdown totals). It also carries one entry per attempted lane in
`lane_ordinals`, `lane_pass`, `lane_cleanup_verified`, the lane boot and
shutdown counts, the lane marker hashes, the prepared, exported and final pair
hashes, and `lane_record_sha256`. `sample_count` and `required_run_count` are
fixed at 10, as they are in B7.

## Inputs and submission

The manifest is the T20 TSV. The canonical disk and vars must sit on the same
APFS volume as the queue. Stage external media into the internal cache first:
cross-volume `cp -c` does not clone, and the campaign reports
`preflight-blocked` before any lane. Run these commands from the repository
root at the exact release head. That head must already be pushed and have
green hosted CI. Every manifest path must be absolute and normalized:

```sh
SHA=$(git rev-parse HEAD)
APP=/absolute/path/to/verified/BridgeVM.app          # Release dry-run artifact for $SHA
IMAGE=$HOME/BridgeVM/work/installed-windows.raw      # canonical, never modified
VARS=$HOME/BridgeVM/work/installed-windows-vars.fd   # matching UEFI variables
MANIFEST=$HOME/BridgeVM/manifests/t23-${SHA:0:8}.tsv
umask 077
python3 - "$APP" "$IMAGE" "$VARS" "$SHA" > "$MANIFEST" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, "scripts/live-gates")
from native_snapshot_restore_artifacts import RELATIONS, digest, tree_hash
app, image, variables, sha = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), sys.argv[4]
print(f"app_bundle\t{app}\t{tree_hash(app)}")
for key, relative in RELATIONS.items():
    print(f"{key}\t{app / relative}\t{digest(app / relative)}")
print(f"image\t{image}\t{digest(image)}")
print(f"vars\t{variables}\t{digest(variables)}")
for key, value in (("source_commit", sha), ("app_profile", "release"), ("binary_profile", "release"),
                   ("binary_features", "venus"), ("rust_toolchain", "1.97.0")):
    print(f"{key}\t{value}")
PY
python3 scripts/live-gates/native_snapshot_restore_inputs.py "$MANIFEST" "$SHA"
JOB=t23-${SHA:0:8}-a19-lifecycle-campaign-r1
scripts/live-gates/bridgevm-live submit t23-a19-lifecycle-campaign \
  --sha "$SHA" --input-manifest "$MANIFEST" --job-id "$JOB"
scripts/live-gates/bridgevm-live status "$JOB"     # poll in bounded intervals
scripts/live-gates/bridgevm-live receipt "$JOB"    # strict read once done
```

`bridgevm-live receipt` serves a T23 receipt only when all of these hold:

- the job is in `done`;
- the public receipt equals the private one;
- the job and ledger seals match;
- every retained lane record matches the receipt's lane lists and hashes; and
- no lane holds `prepared-inputs` or `live`.

## Duration

T20 r8 ran one lane, including the per-lane authentication, cloning and
hashing, from 04:55:44 to 05:03:40 UTC, which is 7 min 56 s. Ten lanes are
therefore about 80 minutes; allow about 1.5 hours. A lane that hangs is bounded
by the lifecycle's own per-boot watchdog (1,500 s) and step timeout (240 s).
The first failure ends the campaign, so a worst-case run is about 3 hours. The
queue has no job timeout. The worker waits for the tier's process group until
it exits or is canceled. Canceling a running campaign kills that group before a
receipt is written. The worker then records a missing receipt, which fences the
queue for operator review, as it does for T20.

## Deterministic checks

`tests/integration/a19-lifecycle-campaign-receipt-contract.py` and
`tests/integration/a19-lifecycle-campaign-runner-contract.py` cover these
areas:

- lane order and cleanup ordering;
- no replacement, and 9/10, missing, duplicate and reordered lanes;
- per-lane clone isolation;
- cross-commit and cross-job lane records, and tampering;
- redaction, publication and the strict read; and
- the cleanup fence and every queue dispatcher.

They run in `scripts/check-project.sh` and in the hosted
`t20-receipt-seal.yml` workflow. They add no live sample.
