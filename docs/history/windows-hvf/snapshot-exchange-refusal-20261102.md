# Named refusal when a volume cannot exchange directories (2026-11-02)

Replacing a complete snapshot, or the managed `current` generation, needs an
atomic directory exchange (`renamex_np` with `RENAME_SWAP`). The hosted exFAT
fixture of 2026-10-04 (run 37185314098) refused that replacement with a bare
`Operation not supported (os error 45)`, kept the old destination byte-exact
and left the complete new staging behind. The message named no operation,
and the kept staging later made a retry refuse as a possible earlier export.

Static reading of the retained case record ranks the exchange as the likely
failing call: it is the only step that runs for a nonempty destination
alone, after staging completes, without an error wrapper. That ranking is
not proof; the original errno site stays unidentified.

Publication now turns only an exchange `ENOTSUP` into a typed refusal that
names the destination, the operation, the volume type from `statfs` and
that the destination was left unchanged, and adds guidance: choose an output
path that does not exist yet, or one on an APFS volume. Other errors keep
their raw OS error. When a failed publication cannot clear its own staging,
or the staging path names another directory, the refusal now names that
staging path and why it was kept instead of discarding the reason.
No non-atomic fallback was added: a two-rename replacement would leave a
moment with no destination. Empty and absent destinations are unchanged.

Evidence is automated tests. On the main source, the new cases failed:
4 FAIL / 45 PASS in the creation filter (three new report cases and the
existing exchange case now requiring the typed refusal). With the change,
215 snapshot_pair library tests pass, including a `statfs` volume-type case.
The hosted exFAT runner now records whether a refusal names the exchange.
Its rerun on the sealed head is separate evidence. A19 remains OPEN.
