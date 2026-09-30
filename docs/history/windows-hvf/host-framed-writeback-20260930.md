# Host-framed NVMe write-back records — deterministic

Classification: deterministic development evidence (automated tests, evidence
rank 3). Current criterion wording and product state come from
`capabilities/windows-hvf.json`. No criterion changes.

## The unframed write-back line

The probe prints `NVMe disk written back: PATH (N bytes)` from
`persist_stop_media` as each write completes, before REGS and the final
report's banner. Guest agent output reaches the same part of run.log. A new
contract, `tests/integration/hvf-host-media-readers-contract.py`, gives each
reader logs with a genuine SYSTEM_OFF report and host teardown in which the
host recorded no NVMe write-back. On the unmodified tree the B7 lane
classifier accepted 3 of 6 (the line in agent output, after a carriage return
in agent output, and in the serial tail). The write-back checks of both NVMe
performance tiers, the `nvme_writeback` fields of the installed-boot runner's
agent-service and host pause/resume gates, and the snapshot lifecycle each
accepted 3 of 6 as well.

## Records inside the final report

`final_report.rs` now prints each completed write a second time, as a
`host media: SUBJECT KIND: PATH (N bytes)` record, on the lines right after
the report's stop record and before `exits:`. Nothing guest-derived is printed
between the banner and those records, and the path is escaped to printable
ASCII as guest text is. Existing parsers accept the layout unchanged: they bind
the banner, the stop line right after it and the count right before the serial
tail, and allow host records in between. The unframed lines stay for people
reading the log. `stop_media.rs` now holds the stop-time persistence and its
records; `nvme_persist.rs` is folded into it.

A failed write still panics before the banner. Such a log binds no report, so
every reader fails closed; no failure record is printed.

## One binding

`scripts/live-gates/hvf_host_media.py` reads the run of well-formed records
right after the bound stop record, which must be printable ASCII, as every
host stop reason is. `hvf_terminal_evidence.py --require-nvme-write-back` and
`scripts/hvf-terminal-report.sh --require-nvme-write-back` exit 0 only when
that run holds exactly one `NVMe disk written back` record. The module, the CLI
and the shell binding decide alike on every case in the new contract,
including the framing corners of `hvf-stop-readers-contract.py`. The NVMe tiers
and the snapshot lifecycle call the CLI, the packaged runner calls the shell
binding, and the B7 lane keeps its whole-log count and also requires the bound
record. Every reader now refuses every forged log and accepts the
current-format log.

The old greps also allowed `NVMe second namespace disk written back:`, which
no probe has printed; the NSID 2 target prints as
`NVMe target namespace (NSID 2)`. The binding keeps the effective requirement,
a primary-namespace write-back, so the runner's placeholder-NSID-1
configuration still cannot pass these gates, as before.

## Retained logs

Run logs from probes built before this change carry no `host media: ` records.
Re-running the B7, NVMe, installed-boot or snapshot verifiers over them fails
closed on the write-back check, so those verifiers need a probe built at or
after this change. No check re-reads retained run.logs, and the retained
corpus was not re-read for this change.

## Limit

No live run. The install progress filter
`HvfWindowsInstallSession.isProgressLine` is display only; it still matches
the substring, so the progress pane now shows both lines.
