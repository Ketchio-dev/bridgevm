# Native VMM agent GET and reply-line bound — 2026-10-07

This checkpoint records one deterministic host-service repair. It does not
establish a Windows workload, a physical-HVF run or a release gate. Product
state and all criteria remain owned by `capabilities/windows-hvf.json`.
Integration starts from main `f1afc507`.

## Defect

The product runtime enables the virtio-console agent service, the operator
control file and the shared-folder sync. A host GET request, whether from a
share poll or the control file, is answered by the guest agent with
`GETBEG <path> <total> <nchunks>`, then `GETCHUNK` lines and `GETEND`.

The host reassembler trusted the guest's declared `total`:

- `begin_get` reserved `Vec::with_capacity(total)` from the parsed value.
  The guest controls that value, and a declaration of `usize::MAX` panics
  the VMM process with `capacity overflow`. Large values that do not
  overflow reserve that much host memory.
- `GETCHUNK` appended every decoded chunk with no comparison to `total`, so
  a guest could keep streaming chunks and grow the buffer without limit.
- The reply line framer buffered an unterminated line without limit. The
  operator control-file framer already used the bounded framer.

Unlike `GETBEG`, the chunked command-output path (`OUTBEG`) already rejected
declarations above `MAX_COMMAND_OUTPUT_BYTES` without reserving them.

## Repair

GET reassembly now has a 64 MiB bound, `MAX_AGENT_GET_BYTES`:

- A declared `total` above the bound reserves nothing and accepts no chunk.
  The transfer ends as a short read, which the share sync already refuses to
  adopt and the operator GET reports with `ok=false`.
- A chunk that would take the buffer past the declared `total` is ignored.
- Agent reply lines use the existing bounded framer with a 32 MiB limit
  (`MAX_AGENT_LINE_BYTES`). Longer lines are discarded up to their newline,
  and framing resynchronises there. The guest agent's largest legitimate
  line is a 32 KiB chunk, or a single-line listing well below the limit.

The shared-folder sync's default file limit (8 MiB) sits well inside the
bound.

## Verification

Two new example fixtures cover the repair, in
`agent_console/get_bound_tests.rs`:

- A `usize::MAX` declaration reserves nothing and ignores its chunk. The
  original code panics with `capacity overflow`.
- A three-byte declaration fed 1000 three-byte chunks keeps exactly three
  bytes. The original code keeps 3000.
- 34 MiB of unterminated reply bytes stay within the line bound, and the next
  newline resynchronises framing. Without the bound, the pending line grows
  past it.

The `hvf_gic_boot_probe` example tests pass 492/0, and clippy is clean.

## Boundary

Only GET reassembly and reply-line framing changed. Command-output reassembly, the control-file
reader, share LS/PUT handling and the line framer are unchanged. No live
guest-agent run was performed for this repair.
