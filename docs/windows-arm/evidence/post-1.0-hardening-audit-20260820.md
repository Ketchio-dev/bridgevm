# Post-1.0 hardening audit (2026-08-20)

Audited head: `d0bb825ae5a6ec108bda3272026d54e209750c19` (= `origin/main` =
the `v1.0.0` tag target). Work branch: `agent/bridgevm-1.0-hardening`.
Host: Mac16,9 (M4 Max, 128 GB), macOS 26.5.2 — the Studio queue host, so live
receipts referenced here are first-party.

Note on the checkout state at audit start: local `main` carried one unpushed
commit (`2a51e2d`, superseded by the remote agent branch
`agent/windows-agent-channel-isolation` at `a9d225e`); the hardening branch
deliberately starts from `origin/main`, not local `main`.

## Classification key

`PROVEN` live gate receipt at stated N · `OBSERVED` live single run ·
`TESTED_ONLY` automated tests only · `STATIC_ONLY` code reading only ·
`OPEN` unresolved.

## Baseline: claim versus evidence

| Area | 1.0 claim | Evidence found | Class |
| --- | --- | --- | --- |
| Boot | A1 9/10 | 10/10 live receipt 2026-08-10 | PROVEN |
| xHCI keyboard | A4 | 12/12 reports + Korean round-trip receipt | PROVEN |
| xHCI pointer | "keyboard and absolute-pointer input" proven (STATUS) | HID 10/10 fired, guest reacted **1/10** (b4-pointer-batch-20260817) | **OPEN** |
| NVMe | A16/A19 | positional-I/O tests + t1-restore-boot receipt | PROVEN |
| Vulkan | A2 p50>=30 | 3× live 58.82 | PROVEN (experimental) |
| D3D11 | A3 3/3 | campaign receipt 8430c7d6… | PROVEN (experimental) |
| Glyphs | known defect (README) | title/tab/menu blank; F4 bounded: 2D scanout black under Venus | OPEN (disclosed) |
| Network | NAT proven | net-live-20260724 receipt; breadth (malformed frames, MTU, IPv6) untested | OBSERVED |
| Audio | A5 | live 2026-08-17; `callback_errors=3` at teardown, unclassified | PROVEN + open quality note |
| Clipboard/folder | A6/A7 | live hashes 2026-08-17 | PROVEN |
| TPM/SB | proven set | live receipts | PROVEN |
| Snapshot | A19 | restore-boot receipt with guest-visible marker | PROVEN |
| Installer/update | implied safe | **no checksum, no signature check, no archive audit, deletes old app before verifying new, no rollback** | STATIC_ONLY defect |
| Clean-machine matrix | none claimed | one host model, one macOS build only | UNTESTED |
| Documentation truth | A18 drift-checked | four drift classes found (below) | defective, fixed |

## Findings, by severity

### P0-1 — pointer defect misclassified and undisclosed (fixed this session)

B4 ("Pointer latency measured") asked only that a measurement be *recorded*,
so recording a 1/10 failure satisfied it and the criterion read `PROVEN` while
its own measured text said "Open as a product defect". The generated summary
filters on `release_blocking`, so the defect could never surface, and
STATUS/README listed pointer input among proven surfaces. Fixed in
`0e04490`: B4 is OPEN with a 20/20 reliability statement, carries a
`known_defect` string surfaced by generation, and a PROVEN criterion carrying
`known_defect` now fails validation.

### P0-2 — installer not fail-closed (fixed this session)

`install.sh` at the audited head: picked the first `.tar.gz` on the latest
release (draft/spoofed names accepted), no checksum, no `codesign --verify`,
extraction before any path audit, `rm -rf` of the existing app before the new
one was verified, no rollback, no version pin, and docs advertising the
absent quarantine attribute as a trust property. Fixed in `57cfde8`:
bootstrap + pinned full installer, SHA256SUMS verification, archive audit
(traversal/absolute/symlink/extra entries), signature + identifier + arch
verification, stage→backup→rename with automatic restore, 16 offline fixture
checks wired into `check-project.sh` and CI. Trust root stated honestly in
`docs/install.md` (HTTPS + same-release checksums + ad-hoc seal; no
provenance claim). GitHub artifact attestations remain a candidate upgrade
and are listed in the backlog rather than claimed.

### P0-3 — documentation drift (fixed this session)

Four classes, each now mechanically checked or corrected:

1. `AGENTS.md` asserted "The product is **Engineering Preview**" in the
   present tense after promotion; the forbidden-claim scan did not cover
   `AGENTS.md` and knew no state-dependent wording. Fixed; mutation-verified.
2. `STATUS.md` disclosed an A5–A7/A17/A19 evidence gap that
   `check-capability-evidence.py` (`ACCEPTED = set()`) proved already fixed,
   while omitting the real open B4 gap. Replaced.
3. The capability-matrix prose stated the promotion rule in
   preview-era wording; now state-agnostic and scanned.
4. Assorted non-historical docs (`docs/fast-mode/README.md`,
   `DRIVERS-README.md`, two decision/plan docs, the DMG builder usage text)
   still called the product an engineering preview. Corrected; `preview` as a
   path name (`target/preview/`, `build-preview-dmg.sh`) is documented as a
   path contract, not a state claim.

### P1 — open, not addressed this session (tracked backlog)

| Item | Known facts | Next measurement | Acceptance gate |
| --- | --- | --- | --- |
| B4 pointer root cause | host surface identical landed-vs-lost; pacing falsified; per-interface protocol correct | run t8 batch with `bv-pointer-capture.ps1`; classify loss as input-stack vs application | 20/20 + p95<=250ms receipt (b4-pointer-reliability-gate-20260820.md) |
| Glyph blanks (caption/tab/menu) | body text fixed; synthetic probes pass while real captions fail; 2D scanout black under Venus blocks pixel proof | live failing-scene A/B draw-path instrumentation (context/resource/upload/barrier identity per draw) | 3/3 caption+menu+tab at 3 resolutions × 3 scales, pixel-mask verified, ±10% frame-time budget |
| Audio teardown `callback_errors=3` | occurs at clean shutdown only; counter name may misclassify an expected device-stop | classify each callback error path at teardown with a typed reason | 10× playback+shutdown with 0 unexpected callback errors, or the counter renamed to what it measures with a test |
| Driver/Secure Boot UX | code 52 + SB-blocked testsigning is an explicit failure (a8 evidence) but preflight surfacing is minimal | enumerate SB/TESTSIGNING/driver/cert/problem-code state in diagnostics | preflight report shows exact blocker; no silent BCD/SB mutation; fallback boot preserved |
| Clean-machine matrix | all receipts from one M4 Max / macOS 26.5.2 | install→boot→update→rollback on other Apple Silicon generations | per-cell receipts; untested cells stay UNTESTED |
| Diagnostics export | no allowlisted support bundle exists | design allowlist; mutation test for redaction | export refuses/blanks every forbidden class (disk bytes, vars, vTPM, keys, clipboard, filenames) |
| Network breadth | NAT + throughput only | malformed-frame/fragmentation/MTU/DNS-failure/reset suite against the userspace NAT | parser fuzz corpus reaching the real dispatch paths; IPv6 explicitly unsupported if unimplemented |
| Compatibility matrix | 2 real titles (Vulkan/D3D11) | 20-workload matrix per the master plan | frame-time p50/p95/p99 + result class per workload |
| Running-state suspend | powered-off snapshot only (B5 scope) | design doc first | not exposed in UI until invariants verified |

### Supply-chain and release-workflow risks (open)

- The release workflow builds on `macos-15` hosted runners and checksums what
  it built, but there is no provenance attestation linking the asset to the
  workflow run; `SHA256SUMS` shares the release with the assets it attests.
  Candidate: GitHub artifact attestations + `gh attestation verify` in the
  installer (version-pinned investigation required, not assumed).
- The manual `workflow_dispatch` path and the tag path build from the same
  steps; asset-naming contract is now pinned by the installer
  (`BridgeVM-<tag>.tar.gz`), which the workflow already satisfies.

## Session verification summary

- `scripts/check-project.sh --fast` PASS at baseline and after each docs/
  installer commit (capability-registry step intentionally stale after code
  commits until the A11 reseal; see the sealing procedure below).
- `tests/integration/install-verify-smoke.sh`: 16/16.
- `verify-pointer-click-reliability.sh --selftest`: parser classifies the
  landed and the 2026-08-17 lost-click fixtures correctly.
- `redact-receipt.py --self-test`: 17 checks after allowlist extension.
- Live queue submissions: the initial pair at `c72f638` was cancelled after
  the probe launch was reworked to a detached Win32_Process Create (blocking
  RUN would have starved the agent channel for the whole 240s window). The
  jobs of record are: t0-check `20260820-134138-21199-15834` at `13e83a5`
  (28 sections, sole failure the intentionally stale registry; check log
  SHA-256 `bedd2636…`; it also caught a real 1/419 store-doctor test race at
  `7dfffd1`, fixed in `13e83a5`) and t8-pointer-reliability
  `20260820-134138-21205-18560` at `13e83a5`, running at audit close. Their
  receipts, not this document, are the evidence of what they measure.

## Sealing rule for this branch

Each code commit legitimately leaves the registry stale
(`code_changed_since`). The branch is sealed by: final code head → t0-check
exact-SHA queue job → docs-only commit updating `tested_commit` and the A11
measured entry citing that job → hosted CI green at the docs head.
