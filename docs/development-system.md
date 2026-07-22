# BridgeVM development system

Document status: **Current**
Adopted: **2026-07-22**

BridgeVM development is organized around explicit product gates and evidence,
not percentages or an ever-growing chronological checklist. This process keeps
fast experimental work possible while preventing a successful unit test from
being mistaken for a production claim.

## 1. Source-of-truth order

When documents disagree, use this order:

1. `STATUS.md` — current product gate state;
2. the active workstream plan named by that gate;
3. current engine/security guides;
4. dated live evidence;
5. `PLAN.md` and historical bring-up notes.

History is preserved. Superseding a document means adding a pointer to the new
source of truth or reclassifying it in `docs/document-manifest.tsv`; it does not
mean deleting useful observations.

## 2. Unit of work: a gate packet

Every non-trivial change should belong to a gate packet with these fields:

```text
Gate ID:
Outcome:
Current blocker:
Scope:
Explicit non-goals:
Risk lane: balanced | aggressive | security-critical
Deterministic acceptance:
Live acceptance:
Rollback:
Evidence destination:
Docs to update:
```

The outcome must be guest- or product-visible. “Implement a class” is scope;
“Windows enumerates TPM0 and completes TPM2_GetCapability” is an outcome.

## 3. Gate states

Use only these states:

| State | Meaning |
| --- | --- |
| `OPEN` | Required work is understood but not yet being executed. |
| `IN_PROGRESS` | Code or evidence work is active. |
| `LOCAL_PROVEN` | Deterministic tests pass; no live-product claim is made. |
| `LIVE_PROVEN` | A dated real host/guest receipt satisfies the stated acceptance test. |
| `RELEASE_PROVEN` | The packaged, signed release artifact passes the clean-machine gate. |
| `EXTERNAL` | The next action requires credentials, hardware, licensing, or an external toolchain. |

`BLOCKED` is not a synonym for difficult. Name the missing dependency and use
`EXTERNAL` only when local engineering cannot produce it.

## 4. Evidence ladder

Each result is labelled with the highest rung actually reached:

| Rung | Evidence | Examples |
| --- | --- | --- |
| E0 | Design | Accepted contract, threat model, or implementation plan |
| E1 | Static | Parser, schema, generated table, package inventory |
| E2 | Deterministic runtime | Unit/integration test with synthetic backend |
| E3 | Host preflight | Real host API/backend initialized without a guest claim |
| E4 | Live guest | Dated guest command, trace, screenshot/frame hash, or shutdown receipt |
| E5 | Release artifact | Packaged/signed/notarized artifact on a clean machine |

Evidence may move a gate only as far as its acceptance criteria allow. E2 TPM
FIFO tests do not clear the E4 Windows TPM enumeration gate; an E4 ad-hoc build
does not clear E5 distribution.

## 5. Workstream structure

| Prefix | Workstream | Primary source |
| --- | --- | --- |
| `SEC-` | vTPM, Secure Boot, VM identity, recovery | Windows completion plan and security model |
| `GPU-` | Windows driver, renderer, scanout, title evidence | Windows 3D and performance plans |
| `ENG-QEMU-` | Compatibility Engine | Compatibility guide |
| `ENG-VZ-` | Apple VZ Engine | Fast Mode guide |
| `ENG-HVF-` | custom Windows VMM/device/lifecycle | Windows completion plan |
| `APP-` | macOS UI, packaging, readiness | root status and app tests |
| `DIST-` | signing, notarization, clean-machine release | root status |
| `DOC-` | documentation, evidence indexing, process | this document and documentation index |

Gate IDs remain stable even when implementation details change. STATUS owns the
current state; active plans own detailed task sequencing.

## 6. Change workflow

1. Select one gate and write or update its packet.
2. Resolve the smallest guest-visible contract slice.
3. Add deterministic tests before claiming local completion.
4. Run the narrow tests, then the affected crate/app/integration suite.
5. For live gates, run only from the signed/package path and archive the exact
   configuration, hashes, logs, and guest receipt.
6. Update `STATUS.md` only after the evidence exists.
7. Add new documents to `docs/document-manifest.tsv` and run the documentation
   check.

Do not mix two stateful migrations into one live experiment. For example, a new
TPM state format and a new Secure Boot variable store should each have an
independent rollback and migration receipt before they are combined.

## 7. Definition of done

A gate is done only when all applicable items are true:

- the user-visible acceptance result is satisfied;
- failure behavior is fail-closed where identity or media is involved;
- rollback is documented and tested;
- deterministic regressions pass;
- required live evidence is dated and reproducible;
- the packaged path, not only a developer binary, is verified when the gate is
  release-facing;
- STATUS and the active plan agree;
- historical evidence remains reachable from the documentation index.

## 8. Risk policy

Performance work may use the `aggressive` lane when the switch is reversible,
media is not rewritten merely by selecting it, and the run records all resolved
knobs. Security-critical work has no aggressive bypass: TPM identity, Secure
Boot variables, BitLocker PCR binding, recovery keys, and signing stay
fail-closed.

Any aggressive path must retain a balanced recovery lane until the release
receipt shows it is no longer needed.

## 9. Verification commands

```sh
bash scripts/check-documentation-system.sh
cargo test --workspace
swift test --package-path apps/macos
tests/integration/product-gates-report.sh
```

Run the narrow test first during iteration. The full commands are handoff gates,
not substitutes for live E4/E5 evidence.

For the Secure Boot supply-chain and offline varstore boundary, run:

```sh
tests/integration/hvf-secure-boot-provisioning-smoke.sh
```

It checks the pinned firmware digest and build receipt, validates all four
Microsoft ESL payloads, and proves PK-last/idempotent/conflict-safe
provisioning. It is E2 evidence only; the Windows guest and recovery gates stay
open until their dated live receipts exist.

During the structural-debt refactoring, run the ratchet budget guard:

```sh
scripts/check-refactor-budgets.sh
```

It fails if any file in `scripts/refactor-budgets.tsv` exceeds its recorded line
or `unsafe`-site ceiling. The repository has no hosted CI, so this stands in for
a CI non-increase gate: grow code into extracted modules rather than these
files, and lower a ceiling only after an extraction actually reduces the file.

## 10. Documentation maintenance

`docs/document-manifest.tsv` classifies every Markdown document. The checker
fails if a new document is unclassified, a manifest path is missing, a path is
duplicated, or a superseding document does not exist.

Long logs belong in a dated evidence document. Root README and STATUS stay
short. When their detail is still useful, archive the exact old version before
condensing it—as done for the 2026-07-22 documentation rewrite.
