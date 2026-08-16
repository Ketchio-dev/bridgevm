# BridgeVM development system

Document status: **Current**
Adopted: **2026-07-22**

BridgeVM development is organized around explicit product gates and evidence,
not percentages or an ever-growing chronological checklist. This process keeps
fast experimental work possible while preventing a successful unit test from
being mistaken for a production claim.

## 1. Source-of-truth order

When documents disagree, use this order:

1. `capabilities/windows-hvf.json` — machine-readable capability state,
   release blockers and the exact user-facing wording;
2. `STATUS.md` — current product gate state in prose;
3. the active workstream plan named by that gate;
4. current engine/security guides;
5. dated live evidence;
6. `PLAN.md` and historical bring-up notes.

The registry is first because prose drifts. README, STATUS and the capability
matrix carry generated blocks rendered from it by
`scripts/render-capability-status.py`, which also fails when a retracted claim
reappears in the code or docs it watches. Change the registry, then regenerate;
never hand-edit a generated block.

History is preserved. Superseding a document means adding a pointer to the new
source of truth or reclassifying it in `docs/document-manifest.tsv`; it does not
mean deleting useful observations.

## 1a. Working rules and the deterministic check

[`AGENTS.md`](../AGENTS.md) holds the binding working rules: the evidence
hierarchy, the ban on relaxing a criterion to pass it, claim discipline,
fail-closed security, canonical-image safety, structural budgets, commit and CI
discipline, and the 120-second foreground wait limit for long gates.

One command is the gate for all of it:

```sh
scripts/check-project.sh          # full deterministic check
scripts/check-project.sh --fast   # truth/format subset, also used by the harness
```

Live gate scheduling, tiers and retention are described in
[Apple silicon live gates](testing/apple-silicon-live-gates.md).

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

### Measuring before changing

A performance change needs a number from the code that ships, taken before and
after. Four ways that has gone wrong here, each of which produced a wrong
conclusion until it was caught:

- **Benchmarking a paraphrase.** A `String`-based reimplementation of a
  `Data`-based accumulator reported 869 ms for something that costs 9.8 ms.
  Exercise the shipped type.
- **Reading a cold first call.** `drainLines` measured 3.11 ms once and 0.90 ms
  warm, and the warm figure is what the product pays. Take a distribution.
- **Quoting an absolute where only a ratio holds.** The framebuffer-read
  comparison gave 5.04 ms against 0.97 ms once and 2.38 against 0.53 later; the
  optimisation did not change, the page cache did. Say which part is stable.
- **Fixing a hot path nobody consumes.** A 145x PPM decode improvement was
  real and worthless, because the feed it fed was unread. The change that
  mattered was deleting the feed.

### Checking that a default still resolves

An absolute path written into a default is a claim about the machine, and it
expires silently. Two were already wrong when this was first checked: a UEFI
vars path pinned to `Cellar/qemu/11.0.1` on a machine running 11.0.2, and a
Vulkan ICD under `share/vulkan`, which is a symlink into the headers formula and
contains no ICD. Neither failed loudly; they simply resolved to nothing.

Ask of every absolute default whether the file is there right now. It takes one
loop over the tree and it found both. `scripts/check-shell-scripts.sh` rejects
version-pinned Cellar paths, which is the half of this that can be automated;
the rest needs the question asked.

An idea that measures negligible is a result, not a failure: record the number
and move on. Several candidates were rejected that way -- a per-window image
reload at 0.037 ms, a forced redraw at 0.79 percent of a core, a store lookup at
0.03 ms, and workspace test parallelism that saved 3.9 s while failing one run
in ten.

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
or `unsafe`-site ceiling, or if a tracked source file is absent from the TSV at
all: an unlisted file used to be unbounded, so a new 3,000-line module passed
untouched. Grow code into extracted modules rather than these files, and lower a
ceiling only after an extraction actually reduces the file. Bare `mod`, `use`
and `#[path]` lines are not counted, because charging a parent for a split
penalises the one action the ratchet exists to encourage.

### Gates that defend the gates

Several checks exist because a gate that cannot fail is worse than no gate: it
converts an unexamined area into a claim. Each was added after the defect it
describes was observed, not anticipated.

| Check | What went wrong without it |
|---|---|
| `scripts/check-shell-scripts.sh` | Five gate scripts ran `cd "$ROOT"` unchecked with no `set -e`, so a failed `cd` left them checking the wrong tree and still exiting 0. |
| `scripts/check-tests-are-reachable.py` | A test file that no `mod`, `#[path]` or `include!` reaches never compiles, and nothing reports it: a test that does not exist cannot fail. |
| `scripts/check-daemon-dto-decoders.py` | A `CodingKeys` case that no `forKey:` decodes makes the field silently absent; decoding still succeeds and the UI shows a default. |
| `scripts/check-swift-force-casts.py` | `try!` and `as!` crash the app instead of surfacing an error. |
| The README disclosure in `scripts/check-attribution-honesty.sh` | Deleting the known-defect paragraph passed every other gate. |

When adding one, mutate it in both directions before trusting it: inject the
defect and watch it fail, then remove the defect and watch it pass. A gate whose
failing direction was never exercised has not been shown to work.

Two checks are deliberately outside both `scripts/check-project.sh` and CI,
because each needs hardware neither has:
[`scripts/check-hvf-windows-p3-gpu-readiness.sh`](../scripts/check-hvf-windows-p3-gpu-readiness.sh)
and
[`scripts/check-hvf-windows-viogpu3d-package.sh`](../scripts/check-hvf-windows-viogpu3d-package.sh)
inspect a real Windows guest and its driver package. Everything else that starts
with `check-` runs in one or the other; a new gate that runs in neither is
enforced only by memory.

Hosted CI runs in `.github/workflows/ci.yml` and is authoritative. Check it
after pushing — `gh run list --limit 1` — and fix a red run before continuing.
A failure in an area you did not touch is still yours to triage: reproduce it
locally before dismissing it as flaky, because "unrelated" and "intermittent"
look identical from a single failed run.

## 10. Documentation maintenance

`docs/document-manifest.tsv` classifies every Markdown document. The checker
fails if a new document is unclassified, a manifest path is missing, a path is
duplicated, or a superseding document does not exist.

Long logs belong in a dated evidence document. Root README and STATUS stay
short. When their detail is still useful, archive the exact old version before
condensing it—as done for the 2026-07-22 documentation rewrite.

### Investigations: iterate in the working tree, commit conclusions

A live investigation moves through hypotheses, and most of them are wrong.
Retracting a published claim is correct and must never be discouraged. What is
wrong is committing each swing separately: the 2026-07-31 `vkCreateInstance`
investigation produced eight `docs(gpu)` commits on one file, adding 425 lines
and deleting 94 to leave 343 — nearly half the churn was the history arguing
with itself, and the useful result was one document.

So, for evidence documents:

- Keep revising the working copy while an investigation is live. Commit when a
  question is settled, not at each turn of reasoning.
- One commit per *conclusion*, not per experiment. "Ruled out X, narrowed to Y"
  is a conclusion; "tried X" is not.
- If a claim in an already-pushed document turns out to be wrong, correct it in
  place and say so plainly in the next commit message. Do not rewrite pushed
  history to hide the error.
- The document itself should read as current findings, not as a diary. Record
  disproven hypotheses because they stop others from repeating the work — but
  as a short "ruled out" section, not as narrative layers.

Status documents follow from this: when an investigation shows that a dated
receipt no longer reproduces, say so in `STATUS.md` next to the claim rather
than leaving the receipt to imply current health.
