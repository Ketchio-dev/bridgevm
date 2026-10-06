# PC NVMe MSI-X table-unmask replay — 2026-10-05

Integrated source checkpoint `dc624e49541931da964fc6fb7e97831c77473691`
repairs deferred NVMe completion notification in the BridgeVM-PC board model.
The existing VirtPlatform path already performs this pending-vector flush.

## Reproduced completion and masking state

Four unchanged public-MMIO fixtures use owned guest RAM and the model's message
drain. A valid Identify command completes while vector0 is masked: its CQE
reports success, PBA bit0 is set, and no message is queued. Unmasking the MSI-X
table entry through a BAR write should then queue exactly one message without
additional submission-queue work. Old production queues none.

The baseline on `3692db697255b04f53c911ff8c222055caf16c51` gives 3 PASS / 1
FAIL. Function-mask blocking, no-pending unmask and direct unmasked completion
controls pass. Masked writes and reads remain quiet; the positive case also
requires no duplicate message after repeated unmask. Baseline raw SHA-256:
`38f18bfa91393bd32912072fc4ffb1db165017209a189795fd6853beff7865c4`.

## Existing flush connected to the BAR path

`crates/bridgevm-hvf/src/platform_pc_nvme_process.rs` extracts the unchanged
queue-processing tail and adds the existing pending MSI-X flush. The PC BAR
write path invokes this helper after its existing MMIO write. Controller
queues, reset behavior, generic MSI-X masks and ECAM unmask handling are
unchanged; this is scoped to the PC NVMe endpoint integration.

All four regression fixture bodies are byte-identical between baseline and
repair, SHA-256
`58ab9832745f1f24f9064f21605fbf21a1192456b19a280bd23174bd54497c97`.
The complete PC platform filter passes 16 tests in debug and 16 in release.
Debug raw SHA-256:
`32f1ca895b05017bd71fddd1a45303a00cfaaac935301b7e0341de2db82c12f6`;
release raw SHA-256:
`602410f327c0fe11c2f9fd511d2a8fb78067a1c853e4d8764c9e2a46eefde93e`.
Focused Clippy, formatting, structural budgets and whitespace checks pass.
Independent source and retained-evidence review does not replay the tests.
The existing NVMe integration ceiling decreases from 15 to 12; the new helper
and test module are registered at actual counted sizes 10 and 152.

NVM Express 1.4 section 7.5.2 requires pending MSI-X notification when both
the function and vector masks clear. This restores those model semantics
without adding an intentional machine-contract deviation. See the
[primary specification](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf),
printed page 295.

## Evidence boundary

These fixtures inspect CQEs, PBA and model messages, not physical MSI delivery.
No VM, Windows boot, guest workload or hardware criterion was executed for this
repair, and no live failure is attributed to it. Complete combined local project
and exact-SHA GitHub-hosted checks remain pending. All 29 criterion states,
thresholds, known defects and product wording remain unchanged; no performance
improvement or criterion/release promotion is claimed.
