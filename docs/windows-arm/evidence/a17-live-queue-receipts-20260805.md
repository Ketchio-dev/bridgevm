# A17: what the local live queue actually sealed (2026-08-05)

## Why this document exists

A17 quoted a job id and two SHA-256 values against
`docs/testing/apple-silicon-live-gates.md`. That document describes how the
queue works; it does not contain those figures, so the claim could not be
checked from the repository. The job is still on disk, and this records what it
holds.

## The cited job

`20260805-115614-86173-14911`, tier `t1-snapshot`, under
`~/BridgeVM/live-queue/done/`. Its `receipt.public.json` reports `outcome:
completed`, `pass: true`.

The sealed pair, read back from the job directory:

```
disk_sha256 6fef8f980adbc6af1ed41a1934d1ebafd93b8a1b4d2ab9a3f8bd07ae950d9b54
vars_sha256 d329b5b03cc5673435d3949f1b5b5b2bfe3f4dfe1f6f0b56ca5894f3bce87d3f
```

Both match what A17 claimed. One naming correction: the receipt field is
`disk_sha256`, not `image_sha256` as the criterion's text had it.

## Job count

A17 said the queue had run 29 jobs. The `done/` directory now holds **121**
completed jobs, so the figure was true when written and has since been overtaken
rather than being wrong. A17's text is updated to cite the count at the time of
this document instead of freezing a number that only ever decreases in accuracy.

## What is not re-verified here

The claim that submission returns immediately, and the nine defects A17 says the
queue surfaced, are not re-measured by this document. It covers the receipt
sealing only, which is the part whose figures were uncited.
