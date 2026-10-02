# Forty-minute continuation boundary repairs — 2026-10-02

Evidence rank: deterministic tests and static reading. Seven confirmed defects
were corrected after the previous clean head `38d424e1` passed hosted full run
`37034991472`. That green result covers only the preceding source. The latest
integrated full check remains pending; no live guest, release proof, criterion
promotion or acceptance-threshold change follows.

## Confirmed failures and corrections

- Tail readers now handle replacement/rotation without stale offset retention;
  24 focused tests passed. FIFO handling was examined during self-review.
- Absent Unicode output aliases could bypass cross-slot media protection.
  The failing reproduction is retained; 27 storage tests and strict Clippy
  passed after alias admission was corrected.
- Receipt authentication could observe rewritten evidence between reads.
  Six mutation tests, eight identity, nine guest-proof and two selected-media
  regressions passed, together with signed fixture smokes. Signed fixtures
  and synthetic proofs do not demonstrate guest workload success.
- The nested helper's signed identifier was matched by prefix: a genuinely
  signed `dev.bridgevm.product-e2e.mutable` binary passed while its plist kept
  the fixed product identity. Exact-line verification now refuses it; the real
  signed self-test retains valid, legacy-layout and changed-plist cases.
- Three block-device malformed queue cases failed before correction; 18 focused
  tests and strict Clippy passed after checked handling was added.
- Used-entry write failure incorrectly allowed publication of its index. The
  first reproduction had one failure and one pass; both tests now pass while
  withholding index publication when entry writes fail.
- TRNG argument-width handling first produced one pass and three failures.
  The corrected implementation passed 20 protocol tests, two dispatch cases
  and strict Clippy. RND32 uses W1, RND64 uses X1, and FEATURES uses W1;
  unsupported calls remain refused and entropy-provider failure stays closed.

## Contract documentation and limits

The machine-contract text previously called A1/A3 and corrected A12/A13 defects
current blockers even though the capability registry recorded later evidence.
Those claims are now historical, with current truth delegated to the registry;
no criterion state or evidence is rewritten by this documentation correction.

[Arm DEN 0098 v1.0](https://documentation-service.arm.com/static/61dff1202183326f21772c2d)
section 2.4 defines requests beginning at one bit. BridgeVM's zero-bit SUCCESS,
zero data and no provider call is an extension, not a mandated zero-input error
or universal interface guarantee. Sections 1.1.1 and 2.3 define GET_UUID as an
implementation-chosen backend identity. The retained words are BridgeVM's
SecRandomCopyBytes backend association, not a universal service UUID or proof
of security provenance. [Arm DEN 0028C v1.2](https://documentation-service.arm.com/static/5f8edaeff86e16515cdbe4c6)
section 3.1/table 3-1 and DEN 0098 tables 7/9 specify the register widths used
by the corrected dispatch.

[QEMU v9.2 virtio_error](https://github.com/qemu/qemu/blob/v9.2.0/hw/virtio/virtio.c#L3635)
marks a malformed device broken and sets NEEDS_RESET/configuration notification
when VERSION_1 is negotiated. Static inspection of BridgeVM finds refusal of
unreadable rings and device-dependent pending or zero-length completion of
invalid chains, without that recovery transition. This existing guest-visible
recovery difference is registered in the deviations manifest; no new device
policy is implemented here. Normal-driver failure and live behavior are unproven.

Focused checks establish these boundaries only. Failed reproductions stay in
the record; final source SHA and full local/hosted results belong in the PR
checkpoint. No private guest assets or raw logs are committed.
