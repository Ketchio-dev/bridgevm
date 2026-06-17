# Guest Tools Tests

Protocol validation and round-trip coverage starts in
`crates/bridgevm-agent-protocol`.

Current alpha coverage:

- Versioned envelope round-trip serialization
- Authenticated `GuestHello` round-trip serialization with advertised feature capabilities
- Optional command `request_id` round-trip serialization
- Guest IP reporting round-trip serialization
- Host time sync, clipboard, display resize, shared folder, and freeze/thaw command round-trip serialization
- Command result round-trip serialization
- Rejection of unsupported protocol versions
- Rejection of empty capability lists, invalid capability versions, and missing or empty tools auth tokens
- Rejection of empty guest IP reports
- Rejection of invalid P0 command fields
- Host-side `bridgevm-agentd` rejection of wrong tools tokens, duplicate capabilities, unknown capabilities, and capability versions above VM policy
- Host-side `bridgevm-agentd` command authorization from message family to advertised session capability
- Host-side `bridgevm-agentd` pending request tracking for request IDs and matching `CommandResult`
- Host-side `bridgevm-agentd` newline-delimited JSON envelope codec rejection of malformed or invalid frames
- Host-side `bridgevm-agentd` `BufRead`/`Write` frame I/O helpers for round trips, clean EOF, partial frames, invalid JSON, and invalid envelopes
- Host-side `bridgevm-agentd` session bootstrap from the first transport frame, requiring authenticated `GuestHello` and rejecting clean EOF or non-hello first frames
- Per-VM guest tools token metadata created with VM bundles and reused for API/CLI `GuestHello` acceptance
- API/CLI guest-tools status, developer token inspection, and explicit `GuestHello` acceptance requests backed by the manifest-derived `bridgevm-agentd` policy
- CLI generation of manifest-compatible `bridgevm-tools-linux` scaffold commands for device and development socket transports, including token-file paths and policy-derived capability flags
- Compatibility Mode QEMU command planning for the virtio-serial guest-tools channel without leaking token values into process arguments
- Runner metadata discovery of guest-tools transport, channel, socket path, and token metadata path
- `bridgevmd` supervisor bootstrap of an authenticated `GuestHello` over the planned guest-tools socket
- Bounded `bridgevmd` drain of heartbeat, guest IP, and guest metrics frames into guest-tools runtime metadata
- Daemon-owned host-to-guest command dispatch over the authenticated guest-tools stream, including pending `request_id` tracking through matching `CommandResult` frames and ignoring unmatched results without overwriting latest-result metadata
- CLI wrappers for common daemon-owned host commands: clipboard sync, display resize, time sync, and shared-folder mount/unmount
- `bridgevm-tools-linux` guest-side scaffold transport selection, handshake, capability override parsing, configurable initial status frames, token file parsing and empty-token rejection, command-result replies, simulated freeze/thaw boundary state tracking, real fsfreeze opt-in parsing/backend dispatch scaffolding, long-running command loops, and fake Unix socket command round trips
- Application-consistent snapshot preflight use of `fs-freeze`/`fs-thaw` capability names, daemon-owned freeze/thaw dispatch around snapshot creation, default simulated freeze/thaw boundaries, and real fsfreeze coverage through fake backend tests plus the separate opt-in live smoke

Application-consistent freeze/thaw remains conservative test coverage. Tests
assert that snapshot preflight records required, advertised, and missing
`fs-freeze`/`fs-thaw` capabilities, and daemon-owned socket tests exercise
request-correlated `FreezeFilesystem`/`ThawFilesystem` dispatch around snapshot
creation. By default, those acknowledgements are simulated in-memory boundary
state.

Real fsfreeze mode is opt-in and should be tested with fake backends unless a
separate live test explicitly opts into touching a real mount. Coverage should
verify that `--real-fsfreeze` requires at least one `--fsfreeze-mount <path>`,
that only the allowlisted mount paths are passed to the backend, that freeze
operations are unwound with matching thaw attempts after partial failure, and
that thaw is attempted after a failed snapshot step. Tests must not claim
database flushing, application quiescing, or general OS/app consistency. Live
coverage, if added, may require root or `CAP_SYS_ADMIN` and must account for
unsupported filesystems.

Future transport, clipboard, resolution, shared folder, and real freeze/thaw
integration tests live here.
