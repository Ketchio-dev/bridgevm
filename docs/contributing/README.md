# Contributor evidence guide

Document status: **Current**

Start with the repository-level [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for
setup, project areas, commands, and the pull-request workflow. This guide
explains how to match a BridgeVM claim to the evidence needed to support it.

## Pick the claim before the test

Write the narrow conclusion you expect a change to support. Then choose the
lowest-cost test that could disprove it:

| Claim | Minimum useful evidence | What it cannot prove |
| --- | --- | --- |
| Parser, serializer, or policy behavior | focused deterministic test | a real guest used that path |
| Workspace remains buildable | hosted CI for the exact SHA | Windows booted or rendered |
| One guest scenario can work | retained live single run | a reliability criterion passed |
| Fixed N-run reliability or latency | sealed live gate at exactly N samples | broader unmeasured compatibility |
| Public release behavior | clean-machine artifact/install gate | kernel-driver signing provenance unless separately verified |

If a run fails before reaching the phenomenon under test, classify that failure
at the layer where it occurred. For example, an all-black or renderer-poisoned
lane is a rendering/package regression, not a pointer result.

## Preserve reproducibility

- Record the exact source commit and binary, image, vars, package, and manifest
  hashes used by a live run.
- Keep every failed experiment in the record and state when a hypothesis was
  refuted.
- Use fresh APFS clones for mutable guest state. Never mutate canonical media.
- Give every parallel lane its own disk and UEFI vars.
- Keep private inputs out of git, CI artifacts, screenshots, and public
  receipts.
- Run long hardware work through the Studio queue; do not attach a self-hosted
  runner to this public repository.

## Change capability wording safely

The source of truth is
[`capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json). Generated
blocks in `README.md` and `STATUS.md` must be regenerated rather than edited into
a different promise.

A criterion changes state only when its existing threshold is met. Do not lower
the threshold, substitute a smoke workload, or use a test-signed graphics
package as A9 production-signing evidence.

After updating the registry, run:

```sh
python3 scripts/render-capability-status.py
python3 scripts/render-capability-status.py --check
python3 scripts/check-capability-evidence.py
```

## Documentation changes

Classify every new file under `docs/` in
[`docs/document-manifest.tsv`](../document-manifest.tsv) as current, active
plan, decision, historical evidence, or reference. Current documents must not
cite a path, command flag, or repository commit that does not exist.

Run:

```sh
bash scripts/check-documentation-system.sh
python3 scripts/check-doc-references.py
```

## Definition of done

A contribution is ready for review when:

1. the diff is focused and contains no temporary instrumentation;
2. relevant focused tests pass;
3. `scripts/check-project.sh` passes;
4. the pull request states the actual evidence level;
5. no private or operator-owned artifacts are staged;
6. hosted CI and Security and quality are green for the reviewed SHA;
7. any live claim links a retained qualifying receipt.

Read [`AGENTS.md`](../../AGENTS.md) for the binding version of these rules.
