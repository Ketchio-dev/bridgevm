# Guest Tools Protocol

The current protocol scaffold lives in `crates/bridgevm-agent-protocol`.
The host-side session trust boundary lives in `crates/bridgevm-agentd`.

All messages are carried inside a versioned envelope:

```json
{
  "protocol_version": 1,
  "message": {
    "Heartbeat": null
  }
}
```

Receivers must reject envelopes with an unsupported `protocol_version`.
`GuestHello.version` is also checked during the alpha period so older guest
tools fail fast when the host protocol changes.

Current alpha encoding is newline-delimited serde JSON. Each frame is exactly
one validated `AgentEnvelope` serialized as JSON followed by `\n`; empty frames,
unterminated frames, multiple frames in one decode call, invalid JSON, and
invalid envelopes are rejected by the `bridgevm-agentd` codec boundary. A future
binary encoding remains open, but the versioned envelope and validated message
contract should remain the same.

`bridgevm-agentd` also exposes `BufRead`/`Write` helpers for transport code.
`read_envelope_line` returns `Ok(None)` only for a clean EOF before any bytes are
read. If EOF arrives after a partial frame without the newline terminator, the
same `MissingFrameTerminator` codec error is returned. `write_envelope_line`
validates the envelope before writing and flushes after the serialized frame is
written.

Session bootstrap is also centralized in `bridgevm-agentd`. Transport code can
call `read_guest_session` to read the first frame and accept it only if it is a
valid authenticated `GuestHello` allowed by the VM policy. A clean EOF before
that first hello is treated as a session setup failure, not as an accepted idle
connection.

The API and CLI expose this policy boundary independently of transport, and the
Compatibility Mode daemon now uses the same session policy when a spawned
guest-tools scaffold connects over the virtio-serial socket.
`bridgevm guest-tools status <vm>` reports the manifest-derived guest tools
requirement, token creation time, and allowed capability versions. Each VM
bundle has a generated tools token stored in
`.vmbridge/metadata/guest-tools-token.json`; `bridgevm guest-tools token <vm>`
prints that developer token during the current scaffold phase. `bridgevm
guest-tools accept-hello <vm> --hello-json <json>` validates a supplied
`AgentEnvelope` against the stored per-VM token and the same `bridgevm-agentd`
`GuestHello` policy used by daemon-owned Compatibility Mode sessions and any
future additional transports.
`bridgevm guest-tools linux-command <vm>` generates a `bridgevm-tools-linux`
scaffold command from that same policy. The generated command includes the
token metadata path, selects the requested device or development socket
transport, and expands the manifest-compatible capability set into
`--capability` flags so users do not manually copy allowed capabilities out of
`guest-tools status`. The daemon socket supports the same requests through the
typed `BridgeVmRequest` protocol.

Compatibility Mode runner plans now reserve the first concrete transport
endpoint. QEMU command previews include a virtio-serial port named
`org.bridgevm.guest-tools.0` backed by
`.vmbridge/metadata/guest-tools.sock`. Runner metadata records that socket path,
the channel name, and the token metadata path so tooling can discover the
transport boundary without placing the token value in process arguments.
When `bridgevmd` owns a spawned Compatibility Mode backend, its supervisor
connects to that socket when it appears, reads the first newline-delimited
envelope with `read_guest_session`, and stores the authenticated session if the
guest presents a valid `GuestHello`. The supervisor then drains bounded batches
of guest-origin envelopes from the long-lived stream. `Heartbeat`,
`GuestIpChanged`, and `GuestMetrics` update
`.vmbridge/metadata/guest-tools-runtime.json`, which is surfaced through
`bridgevm guest-tools status <vm>`. Host-to-guest command dispatch over the same
stream is available through the daemon-owned backend path:
`bridgevm --socket <sock> guest-tools set-clipboard <vm> --text <text>`,
`bridgevm --socket <sock> guest-tools resize-display <vm> --width <w> --height
<h> --scale <scale>`, and `bridgevm --socket <sock> guest-tools time-sync
<vm>`. Shared-folder commands use the manifest-approved share name as the
normal user-facing selector:
`bridgevm --socket <sock> guest-tools mount-share <vm> --name <sharedFolders.name>
[--request-id <id>]` and `bridgevm --socket <sock> guest-tools unmount-share
<vm> --name <sharedFolders.name> [--request-id <id>]`. The host API/CLI
resolves that name through the VM manifest's `sharedFolders` entries and
dispatches `MountShare { name, host_path_token }` with the host-approved
`hostPathToken` or the manifest-derived stable token. Users should not need to
copy or paste raw tokens for the approved-share path. `host_path_token` is a
host-issued opaque token that identifies a host-approved path through the
daemon's shared-folder approval/metadata registry; it is not a serialized path
and the guest must not interpret it as one. The daemon/API/CLI shared-folder
runtime surface reports the currently known session entries by share `name`,
`host_path_token`, and host approval metadata. Host-facing status may include
the approved host path for operator debugging, but that path remains on the
host side of the boundary and must not be presented to the guest as a guest
path or mount instruction. It is a list of alpha scaffold state, not a guest
filesystem mount table. A raw token dispatch path may remain available only as
a developer/debug escape hatch, alongside
`bridgevm --socket <sock> guest-tools send-command <vm> --envelope-json
<json>`.

`TimeSync` is part of this host-to-guest command dispatch surface: the CLI/API
and macOS dashboard can send the current or caller-specified epoch millis when
the authenticated session advertises `time-sync`, then surface the matching
request ID and latest `CommandResult`. That status is command plumbing
readiness, not proof that a production guest clock service changed system time.

The host CLI also exposes high-level wrappers for the alpha drag-and-drop,
application, and window protocol commands:

```bash
bridgevm guest-tools file-drop-start <vm> \
  --transfer-id <id> \
  --file-name <name> \
  --size-bytes <bytes> \
  [--request-id <id>]

bridgevm guest-tools file-drop-chunk <vm> \
  --transfer-id <id> \
  --chunk-index <index> \
  --data-base64 <base64> \
  [--request-id <id>]

bridgevm guest-tools file-drop-complete <vm> \
  --transfer-id <id> \
  [--request-id <id>]

bridgevm guest-tools list-applications <vm> [--request-id <id>]
bridgevm guest-tools launch-application <vm> --id <application-id> [--request-id <id>]
bridgevm guest-tools list-windows <vm> [--request-id <id>]
bridgevm guest-tools focus-window <vm> --id <window-id> [--request-id <id>]
bridgevm guest-tools close-window <vm> --id <window-id> [--request-id <id>]
```

These wrappers select typed protocol messages and optional request correlation;
they do not widen the trust boundary beyond the authenticated guest-tools
session.
Commands with a `request_id` are tracked until the guest returns a matching
`CommandResult`. A matching result means the scaffold accepted and processed
the protocol command; during the current alpha it must not be read as proof that
the guest OS changed state. The daemon also records the most recent matching
result in guest-tools runtime metadata as `last_command_result`, so
`guest-tools status` and the macOS client can show the latest request ID,
capability, success flag, optional error text, and completion timestamp.
Status surfaces should treat that value as the latest diagnostic breadcrumb for
safe alpha command dispatch, not as a durable command log.

The first guest-side executable scaffold is
`runners/bridgevm-tools-linux`. It can connect to the development Unix socket
with `--socket`, or to the in-guest virtio-serial device with
`--device /dev/virtio-ports/org.bridgevm.guest-tools.0`. It reads the per-VM
`guest-tools-token.json` through `--token-file`, sends `GuestHello` plus initial
heartbeat/IP/metrics status, and acknowledges basic host commands with
`CommandResult`. For clipboard alpha smoke tests, it can acknowledge
host-origin `SetClipboard` commands and can emit a guest-origin
`ClipboardChanged` frame when started with clipboard seed text. Both paths are
wire-protocol exercises only: `SetClipboard` does not write to the Linux
desktop clipboard yet, and `ClipboardChanged` is synthetic scaffold state rather
than a real OS clipboard watcher. The scaffold advertises a broad default
capability set for development, or an exact manifest-compatible list with
repeated `--capability <name[:version]>` flags. `bridgevm guest-tools
linux-command <vm>` is the preferred way to create that exact list for either
the virtio-serial device transport or the development Unix socket transport. It
is a protocol loop scaffold; Linux OS clipboard, display, shared-folder, and
most drag-and-drop integration still need real guest backends. Freeze/thaw
defaults to simulated in-memory boundary state, but the Linux scaffold can be
started with `--real-fsfreeze` and one or more `--fsfreeze-mount <path>` values
to route freeze/thaw through an explicit allowlist-backed `fsfreeze` command
backend. That real mode is still not app quiescing: it may require root or
`CAP_SYS_ADMIN`, can fail on unsupported filesystems, and does not flush
databases or coordinate application writes. Application and window metadata are
also protocol alpha surfaces: the Linux scaffold can
advertise `applications` and `windows`, list static scaffold entries, and
acknowledge launch/focus/close commands through in-memory state, but it does
not launch Linux applications or control real desktop windows yet.
Drag-and-drop alpha commands (`FileDropStart`, `FileDropChunk`, and
`FileDropComplete`) track transfer state in memory and acknowledge the command
sequence. When `bridgevm-tools-linux` is started with `--file-drop-dir <dir>`,
the scaffold base64-decodes chunks and writes a completed file into that
explicit output directory after the received byte count matches the declared
size. File names are limited to one safe path component, so absolute paths,
nested paths, and parent-directory traversal are rejected. Without
`--file-drop-dir`, file-drop remains an in-memory protocol exercise. Initial
guest IP and metrics frames can be controlled with
`--guest-ip <addr[@iface]>`, `--no-guest-ip`, `--metrics-cpu-percent`,
`--metrics-memory-used-mib`, and `--no-metrics`; the scaffold suppresses guest
IP or metrics frames when the matching capability is not advertised.

Transport targets:

- virtio-serial for QEMU Compatibility Mode
- vsock for later backend targets

Initial message families:

- Authenticated `GuestHello` and heartbeat
- Host time sync
- Guest IP reporting
- Clipboard sync
- Dynamic display resize
- Shared folder mount/unmount
- Drag-and-drop file transfer
- Application and window metadata
- Guest metrics
- Command results for request/response correlation
- Application-consistent snapshot freeze/thaw scaffold messages, guarded by
  `fs-freeze` and `fs-thaw`, for the daemon-owned snapshot execution
  scaffold.

Safe MVP smoke coverage lives under `tests/integration/guest-tools-*-cli-smoke.sh`.
The core alpha surfaces below are part of
`tests/integration/metadata-safe-smoke-suite.sh`. These smokes use disposable
stores, local Unix sockets, the Linux tools scaffold, fake QEMU process stubs
where daemon-owned runner metadata is needed, and fake/shadowed host helpers
for backend primitives such as `fsfreeze`. They must not boot a real guest,
start a real QEMU or Apple VZ VM, open a GUI console, mutate host networking,
freeze real host mounts, or claim real guest OS state changes.

| Surface | Safe smoke |
| --- | --- |
| Handshake/policy | `guest-tools-handshake-cli-smoke.sh` |
| Clipboard | `guest-tools-clipboard-cli-smoke.sh` |
| Display resize | `guest-tools-display-resize-cli-smoke.sh` |
| Shared folders | `guest-tools-shared-folder-cli-smoke.sh` and `shared-folder-manifest-cli-smoke.sh` |
| File drop | `guest-tools-file-drop-cli-smoke.sh` |
| Metrics | `guest-tools-metrics-cli-smoke.sh` |
| Agent update notice | `guest-tools-agent-update-cli-smoke.sh` |
| Command tracking | `guest-tools-command-tracker-cli-smoke.sh` |
| Time sync | `guest-tools-time-sync-cli-smoke.sh` |

These tests verify protocol framing, policy capability exposure, authenticated
session acceptance, request/result correlation, passive runtime metadata, and
the Linux scaffold's opt-in command backends. Passing them is not proof that a
production Linux clipboard service, display server, filesystem mount, updater,
or drag-and-drop integration changed live guest state.

`GuestHello` is the first trusted session boundary. The guest advertises its OS,
optional agent version, one or more feature capabilities, and a per-VM tools
auth token:

```json
{
  "protocol_version": 1,
  "message": {
    "GuestHello": {
      "version": 1,
      "guest_os": "linux",
      "agent_version": "1.0.0",
      "capabilities": [
        { "name": "heartbeat", "version": 1 },
        { "name": "clipboard", "version": 1 }
      ],
      "auth": {
        "kind": "tools_token",
        "token": "per-vm-token"
      }
    }
  }
}
```

`bridgevm-agentd` validates that first envelope before any guest command is
trusted. It requires `GuestHello`, checks the protocol fields, compares the
tools token with the VM policy, rejects duplicate or unknown capabilities, and
rejects capability versions newer than the host policy allows. Auth is optional
in the wire shape for forward-compatible decoding, but a validated session
requires it.

After the session is accepted, `bridgevm-agentd` authorizes each message against
the advertised session capabilities:

| Message | Required capability |
| --- | --- |
| `GuestHello` | none, pre-session handshake |
| `Heartbeat` | none, base liveness |
| `CommandResult` | none, request/response plumbing |
| `TimeSync` | `time-sync` |
| `GuestIpChanged` | `guest-ip` |
| `ClipboardChanged`, `SetClipboard` | `clipboard` |
| `ResizeDisplay` | `display-resize` |
| `MountShare`, `UnmountShare` | `shared-folders` |
| `FileDropStart`, `FileDropChunk`, `FileDropComplete` | `drag-drop` |
| `ListApplications`, `LaunchApplication` | `applications` |
| `ListWindows`, `FocusWindow`, `CloseWindow` | `windows` |
| `FreezeFilesystem` | `fs-freeze` |
| `ThawFilesystem` | `fs-thaw` |
| `GuestMetrics` | `guest-metrics` |
| `AgentUpdateAvailable` | `agent-update` |

Agent update messages are passive status notices only. `AgentUpdateAvailable`
is authorized by the separate `agent-update` capability, which host policy
exposes when the VM manifest enables `security.signedAgentUpdates`. The daemon
may record and report the current version, available version, download URL,
signature metadata, and observed timestamp, but it must not download, verify,
install, execute, or claim completion of a guest-tools update from this notice.

Application-consistent snapshot coordination is also still a conservative
scaffold/boundary. The storage/API/CLI preflight requires the `fs-freeze` and
`fs-thaw` capability names and records whether an authenticated guest-tools
session has advertised them. The protocol surface may carry `FreezeFilesystem`
and `ThawFilesystem` messages, and the Linux tools scaffold may acknowledge
them for socket-level testing.

By default, Linux tools freeze/thaw acknowledgements are simulated in-memory
state. When the runner is started with `--real-fsfreeze` and at least one
`--fsfreeze-mount <path>`, the scaffold may call the Linux `fsfreeze` command
backend for those explicitly allowlisted mounts. That opt-in path is a
filesystem freeze primitive only: it may require root or `CAP_SYS_ADMIN`, may
be unsupported by a filesystem or mount state, and does not flush databases,
quiesce applications, coordinate app writes, or prove application consistency.
`snapshot create <vm> <name> --kind application-consistent` remains a
readiness/preflight record only. As the boundary matures, the expected sequence
is:

1. Validate the authenticated guest-tools session.
2. Require advertised `fs-freeze` and `fs-thaw` capabilities.
3. Send a request-correlated freeze command and wait for `CommandResult`.
4. Attempt the disk snapshot boundary.
5. Always send the matching thaw command after the snapshot attempt, including
   after snapshot failure, and persist the outcome for diagnostics.

Smoke coverage should follow the existing live Unix socket harnesses in
`tests/integration`: run `bridgevm-tools-linux --socket`, send typed frames
through the daemon-owned path, and assert that the default path reports the
simulated scaffold boundary where no OS `fsfreeze` was executed. It should also
cover both the successful freeze/thaw sequence and the failure path where thaw
is still attempted. The metadata-safe backend smoke must shadow `fsfreeze` on
`PATH` with a fake executable and use ordinary temporary directories for
ordering and rollback assertions. A test that targets a real mount or requires
host privileges belongs in a separate opt-in live harness and must document
those requirements.

Host commands may include an envelope `request_id`. `bridgevm-agentd` records
those commands as pending after authorization, rejects duplicate pending request
IDs, and accepts `CommandResult` only when its `request_id` matches a pending
command. Commands without a `request_id` are treated as fire-and-forget and do
not create pending state. `bridgevmd` uses this tracker for commands written to
an authenticated guest-tools stream. `CommandResult` reports protocol-level
completion for that command and may carry guest-side success or failure text,
but it is not a durable host assertion that a Linux desktop integration has
completed. The latest matching result is persisted under runtime
`last_command_result` for status surfaces; it is a diagnostic breadcrumb, not a
full command history. For alpha clipboard smoke tests, a successful result for
`SetClipboard` means the Linux scaffold decoded the command and sent the
acknowledgement.

Clipboard alpha uses the same authenticated stream in both directions:

- `SetClipboard` is a host-to-guest command authorized by the `clipboard`
  capability. It may include a `request_id`; when it does, the host keeps the
  request pending until a matching `CommandResult` arrives.
- `ClipboardChanged` is a guest-to-host event authorized by the same
  capability. In the Linux scaffold it is emitted only from supplied scaffold
  state for smoke testing, not from an OS clipboard monitor.

The current Linux tools scaffold therefore verifies envelope encoding,
capability authorization, request correlation, and daemon plumbing for
clipboard sync. Real reads from and writes to the Linux OS clipboard remain a
future backend integration.

Drag-and-drop alpha is exposed through `FileDropStart`, `FileDropChunk`, and
`FileDropComplete`, or through the matching high-level CLI wrappers:
`bridgevm guest-tools file-drop-start <vm> --transfer-id <id> --file-name
<name> --size-bytes <bytes> [--request-id <id>]`, `bridgevm guest-tools
file-drop-chunk <vm> --transfer-id <id> --chunk-index <index> --data-base64
<base64> [--request-id <id>]`, and `bridgevm guest-tools file-drop-complete
<vm> --transfer-id <id> [--request-id <id>]`. The Linux scaffold validates the
transfer sequence and stores transient transfer metadata in memory. If
`--file-drop-dir <dir>` is configured, it decodes chunk payloads into file
bytes and writes the completed file under that explicit directory using a safe
single-component file name. Without that option, it does not update guest
filesystem state.

Application/window metadata alpha follows the same request/response boundary.
`ListApplications` and `ListWindows` return scaffold-owned entries only, and
`LaunchApplication { id }`, `FocusWindow { id }`, and `CloseWindow { id }`
validate the supplied ID against that scaffold state before returning
`CommandResult`. A successful result means the authenticated scaffold decoded
and accepted the protocol command. It must not be presented as evidence that a
Linux process was launched, a real window was focused, or a real window was
closed.
Because list requests do not mutate guest state, they are the preferred safe
alpha actions for dashboard status controls while real application and window
integration is still absent.
The matching high-level wrappers are `bridgevm guest-tools list-applications
<vm> [--request-id <id>]`, `bridgevm guest-tools launch-application <vm> --id
<application-id> [--request-id <id>]`, `bridgevm guest-tools list-windows <vm>
[--request-id <id>]`, `bridgevm guest-tools focus-window <vm> --id <window-id>
[--request-id <id>]`, and `bridgevm guest-tools close-window <vm> --id
<window-id> [--request-id <id>]`.

Shared-folder alpha is also a protocol-level exercise. The host sends
`MountShare { name, host_path_token }` or `UnmountShare { name }`, both
authorized by the `shared-folders` capability. `name` identifies the approved
share from the VM manifest's `sharedFolders.name` field and the session entry
seen by the guest tools scaffold. For the high-level API/CLI path, callers pass
that approved share name and host-side code looks up the matching manifest
entry before dispatching `MountShare` with the approved `host_path_token`.
`host_path_token` is the host-issued opaque token for a path that has already
been approved on the host, either explicitly via `hostPathToken` or by the
manifest-derived stable token. The token is resolved only by host-side
registry/metadata code. The guest tools scaffold must treat it as an identifier
to echo through the protocol boundary, not as a path to interpret or mount
directly.

In the Linux scaffold, `MountShare` records or updates an in-memory share entry
for `name` with the supplied `host_path_token`. `UnmountShare` removes that
in-memory entry. No Linux mount, bind mount, FUSE mount, virtiofs attachment, or
guest filesystem path is created during this alpha. If a shared-folder command
has a `request_id`, the scaffold returns a `CommandResult` after updating that
in-memory state; without a `request_id`, the command remains fire-and-forget and
no result is emitted. A successful result therefore means only that the
authenticated scaffold accepted the command and updated its transient
shared-folder state.

When the daemon records shared-folder runtime state for status/list APIs, it
must preserve that same boundary. `guest-tools status` and any shared-folder
list output may expose the active in-memory session entries so operators can
debug the command path. Host-facing status may include the approved host path,
the `host_path_token`, approval timestamps, approval source, and related
metadata from the host registry. The guest-facing protocol still carries only
the opaque token and share name; the guest must not resolve, canonicalize, or
trust a host path string. These entries are not durable VM configuration and
must not be presented as OS mounts, bind mounts, virtiofs exports, or guest
paths. If the guest-tools session disconnects or a VM restarts, the alpha
runtime list can disappear unless a future backend explicitly rehydrates it
through a real shared-folder integration.

Guest IP reports use `GuestIpChanged`:

```json
{
  "protocol_version": 1,
  "message": {
    "GuestIpChanged": {
      "addresses": [
        {
          "address": "192.168.64.2",
          "interface": "eth0"
        }
      ]
    }
  }
}
```

The alpha validator rejects empty guest IP reports and unspecified addresses
such as `0.0.0.0` or `::`.
