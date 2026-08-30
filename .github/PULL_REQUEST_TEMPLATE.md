## Outcome

What user-visible or engineering conclusion does this change establish?

## Validation

List the exact commands, hosted runs, or retained live receipts used. State the
evidence level: static reasoning, deterministic test, live single run, or fixed
sample-count live gate.

## Risk and boundaries

Describe security, private-data, compatibility, machine-contract, packaging,
and rollback impact. Write “none” only after checking each category.

## Checklist

- [ ] I read `AGENTS.md` and did not weaken an existing criterion.
- [ ] A user-approved `PLAN.md` covers this change if it spans three or more files.
- [ ] Focused tests pass, and `scripts/check-project.sh` passes before merge.
- [ ] Capability wording comes from `capabilities/windows-hvf.json` where applicable.
- [ ] New or revised documentation is classified and its links resolve.
- [ ] No Windows media, VM disk, vars, vTPM state, recovery key, proprietary title content, private receipt, `GOAL.md`, `PLAN.md`, or `HANDOFF.md` is staged.
- [ ] Test-signed graphics inputs have not been presented as A9 production-signing evidence.
- [ ] Temporary instrumentation has been removed or explicitly scoped as retained diagnostics.
