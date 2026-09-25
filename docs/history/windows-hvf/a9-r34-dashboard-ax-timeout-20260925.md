# A9 r34 dashboard Accessibility timeout — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`; A9 remains OPEN and the product remains
ENGINEERING_PREVIEW. No clean-machine ISO or import journey is proven here.

## Sealed observation

The exact-main source was `bc5f11e53f704f156016ebf65d9fc150f56b3189`
(M1). The input manifest SHA-256 was
`09518a48f9a58b5028faa943e5b4e64740d1e0347ba6b1011252eae7dd4aed44`.
Physical job `t17-bc5f11e5-first-ready-diagnostic-r34` ended failed with exit
status 1. Its public and private strict T17 receipts were byte-identical at
SHA-256 `373d557f50d6b6a601218c71ecb7ef55a81938ace1fc26ec2f8f466a4d5f19dd`.
Both official receipt validators passed. This proves the receipts are valid;
their result is failure: one run, zero first READY passes,
`criterion_pass=false`, and `capability_promotion=false`. Worker cleanup was
verified.

The authenticated lane result SHA-256 was
`f12d72ca4741066753aa994c7bb1a8733ca1d898de65a9879feb0eb5a2750410`.
It recorded `failure_code=ui-element-missing` and
`required accessibility identifier was not found:
bridgevm.dashboard.advanced; windows=0 timeout_s=60.0`. The lane recorded VM
creation, Windows installation and Secure Boot provisioning as complete, then
failed while looking for `bridgevm.dashboard.advanced`, before pressing
`bridgevm.windows.runtime.start` and before first READY. `windows=0` is the
final AXWindows count, not proof that the app crashed. This differs from r33,
which reached a later first-boot boundary without READY/PONG and had an
incomplete host-stop record. Neither run proves an A9 journey.

The lane recorded final disk SHA-256
`e442a89c2e4cfbecca3206108a881c7e801ddd40d27e039a9302ecd073759ba0`
and final vars SHA-256
`50b92f5ca65fde2d6ea324941fd3a5640bc1785ef902ab6782b5de422a2d5785`.
These are lane-only recorded values: the public failed-run receipt reports
final hashes as `absent`, and the cleaned underlying files cannot be rehashed.
The lane's guest-evidence digest matches an unproven nonce sentinel, not a
guest READY or completed journey. No failure-time screenshot, framebuffer,
AX window/process sample, host-stop state, timer or GIC packet was retained.
The private helper log was empty and the scratch tree was removed. The
failure's cause is unknown.

## Deterministic follow-up and limit

Separately committed source `33c016f025a1fd454d2d0af3175f0dfeb2ad4c17`
(S, parent M2 `5290c561b8664518c8b34ff6a9149568e4afe247`) records bounded,
path-free AX role/identifier, raw AX error and process-liveness observations
when this identifier lookup times out. It preserves the 60-second timeout and
`ui-element-missing` failure. The first focused compile failed on a String to
CFString conversion; the failed log SHA-256 is
`07b8d7569028638af6d97b59442394f1bb8ba9b93f2dcf513607f673dada5e30`.
After correction, five focused Swift tests passed (log SHA-256
`9a3e9be7e14c202522dbe4e02ae633cf071deb53f4a8080599c84b27261fe7f8`),
and structural budgets passed (log SHA-256
`1c6f98de106b54f3478f7d776cf37c2a6b7b115eb9fff1982e67d2fbdb8e9d4a`).
These deterministic checks do not show that a later physical run will capture
the missing state or complete a guest journey. Exact integrated-head hosted
verification and a fresh sealed physical diagnostic remain separate gates.
