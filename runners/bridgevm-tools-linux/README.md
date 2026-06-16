# BridgeVM Linux Tools Scaffold

`bridgevm-tools-linux` is the first guest-side BridgeVM Tools executable. It is
a protocol scaffold for development, not a complete Linux integration daemon
yet.

The runner connects to either a development guest-tools Unix socket or the
Linux virtio-serial device, sends an authenticated `GuestHello`, emits an
initial heartbeat, guest IP, metrics, and optional scaffold clipboard status
batch, then keeps reading host commands until the transport reaches EOF.
Commands with a `request_id` receive a `CommandResult`. Use `--serve-once` for
tests or manual smoke checks that should exit after one host command.

Example development socket flow:

```bash
bridgevm-tools-linux \
  --socket <vm-bundle>/metadata/guest-tools.sock \
  --token-file <vm-bundle>/metadata/guest-tools-token.json
```

Example in-guest virtio-serial flow:

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file /path/to/guest-tools-token.json
```

Use either `--socket` or `--device`, not both.

For VM-specific smoke tests, prefer generating the scaffold invocation from the
host policy instead of copying token paths and capability flags by hand:

```bash
bridgevm guest-tools linux-command <vm>
```

`bridgevm guest-tools linux-command <vm>` prints a `bridgevm-tools-linux`
command line for the selected transport. The generated command uses the
manifest-derived guest-tools policy, includes the per-VM
`guest-tools-token.json` path, and expands the allowed capabilities into
repeated `--capability <name[:version]>` flags. It can generate the in-guest
virtio-serial device form and the development socket form, so the resulting
command stays compatible with `GuestHello` validation without manually copying
capability lists from `guest-tools status`.

Prefer `--token-file` over `--token`. Passing the token directly is useful for
small local smoke tests, but command-line arguments can be visible to other
processes. The scaffold accepts BridgeVM's JSON token metadata file or a raw
token text file; empty tokens are rejected before the `GuestHello` is sent.

By default the scaffold advertises the core development capability set:
`time-sync`, `guest-ip`, `clipboard`, `display-resize`, `shared-folders`, and
`guest-metrics`, plus the protocol alpha capabilities `drag-drop`,
`applications`, `windows`, `fs-freeze`, and `fs-thaw`. If a VM manifest
disables an integration, pass an explicit capability list that matches the host
policy, or use
`bridgevm guest-tools linux-command <vm>` to generate one:

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --capability heartbeat \
  --capability guest-ip \
  --capability time-sync \
  --capability guest-metrics
```

Capability versions default to `1`; use `--capability clipboard:1` to make the
version explicit.

Initial telemetry is also configurable. The scaffold always sends `Heartbeat`.
It sends `GuestIpChanged` only when `guest-ip` is advertised, and
`GuestMetrics` only when `guest-metrics` is advertised.

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --guest-ip 192.168.64.10@enp0s1 \
  --metrics-cpu-percent 17 \
  --metrics-memory-used-mib 1024
```

Use `--no-guest-ip` or `--no-metrics` to suppress those optional status frames.

Clipboard alpha is also configurable for protocol smoke tests. Passing
`--clipboard-text <text>` sends an initial guest-origin `ClipboardChanged`
frame after `GuestHello` when the `clipboard` capability is advertised:

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --clipboard-text "hello from guest scaffold"
```

This is synthetic scaffold state. The runner is not watching the Linux desktop
clipboard, and the frame does not prove that a user copied text inside the
guest.

Current supported command acknowledgements:

- `TimeSync`
- `SetClipboard`
- `ResizeDisplay`
- `MountShare`
- `UnmountShare`
- `FileDropStart`
- `FileDropChunk`
- `FileDropComplete`
- `ListApplications`
- `LaunchApplication { id }`
- `ListWindows`
- `FocusWindow { id }`
- `CloseWindow { id }`
- `FreezeFilesystem { timeout_millis }`
- `ThawFilesystem`

For `SetClipboard`, acknowledgement means the scaffold decoded the host command
and wrote a `CommandResult` for the same `request_id`. It does not write the
text into the Linux OS clipboard yet. If the host sends a command without a
`request_id`, the scaffold treats it as fire-and-forget and does not emit a
`CommandResult`; with a `request_id`, `bridgevm-agentd` keeps the command
pending until the matching result arrives.

Linux clipboard writes are available only through an explicit command backend.
Start the runner with `--clipboard-command <path>` to route host-origin
`SetClipboard { text }` commands into an external program:

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --clipboard-command /usr/local/bin/bridgevm-set-clipboard
```

The scaffold writes the clipboard text to the command's standard input and
uses the command exit status to produce the correlated `CommandResult`.
Successful exits acknowledge the write; failed starts, failed stdin writes, or
non-zero exits return a failed `CommandResult` when a `request_id` is present.
Without `--clipboard-command`, `SetClipboard` keeps the default scaffold
acknowledgement behavior and does not modify the Linux OS clipboard.

Linux display resize is also available only through an explicit command
backend. Start the runner with `--display-resize-command <path>` to route
host-origin `ResizeDisplay { width, height, scale }` commands into an external
program:

```bash
bridgevm-tools-linux \
  --device /dev/virtio-ports/org.bridgevm.guest-tools.0 \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --display-resize-command /usr/local/bin/bridgevm-resize-display
```

The scaffold invokes the command with the requested `width`, `height`, and
`scale` as argv values, then uses the command exit status to produce the
correlated `CommandResult`. Failed starts or non-zero exits return a failed
`CommandResult` when a `request_id` is present. Without
`--display-resize-command`, `ResizeDisplay` keeps the default scaffold
acknowledgement behavior and does not change the Linux display.

The matching host-side shared-folder wrappers use approved manifest share names
as the normal selector:

```bash
bridgevm --socket <sock> guest-tools mount-share <vm> \
  --name <sharedFolders.name> \
  [--request-id <id>]

bridgevm --socket <sock> guest-tools unmount-share <vm> \
  --name <sharedFolders.name> \
  [--request-id <id>]
```

The host CLI/API resolves `<sharedFolders.name>` through the VM manifest's
`sharedFolders` entries and dispatches `MountShare { name, host_path_token }`
with the approved `hostPathToken`, or with the stable token derived from that
manifest entry. This lets users mount by approved share name instead of copying
raw opaque tokens. Direct token dispatch is reserved for developer/debug paths
such as raw envelope tests.

Shared-folder alpha support is intentionally transient. `MountShare { name,
host_path_token }` records or replaces an in-memory share entry keyed by
`name`, and `UnmountShare { name }` removes that entry. `host_path_token` is an
opaque host-issued token for a host-approved path selected by the host from the
manifest-approved share name; the Linux scaffold treats it as an identifier
supplied by the authenticated host, not as a filesystem path to parse,
canonicalize, or mount. These commands require the `shared-folders` capability,
but they do not call the Linux `mount` command, create a bind mount, start a
FUSE server, attach virtiofs, or create a guest filesystem path. When a
shared-folder command includes a `request_id`, the scaffold sends a
`CommandResult` after updating this in-memory state; without a `request_id`, it
remains fire-and-forget.

Host-side status/list output for shared folders should be read as a view of
that same alpha session map plus host registry metadata. A listed share means
the authenticated scaffold has accepted a `MountShare` command for the current
session and has not yet accepted the matching `UnmountShare`. Host-facing status
may show the approved host path, token, approval time, or approval source for
debugging, but those fields are not guest instructions. It does not mean a Linux
mount exists, a path was created in the guest, or the opaque `host_path_token`
was interpreted as a filesystem location.

Drag-and-drop alpha support is available through high-level host wrappers:

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
```

The Linux scaffold treats these as protocol smoke-test commands. It tracks the
transfer ID, expected file metadata, chunk indexes, and completion state in
memory, then returns `CommandResult` when a correlated request is accepted. By
default this remains an in-memory protocol exercise. If the scaffold is started
with `--file-drop-dir <dir>`, it base64-decodes chunk payloads and writes the
completed file into that directory after the received byte count matches the
declared size. The file name must be a single safe path component; nested paths,
absolute paths, and parent-directory traversal are rejected.

Application and window metadata support is also intentionally scaffold-only.
`ListApplications` and `ListWindows` return static or in-memory entries owned
by the runner, not a live inventory from the Linux desktop environment.
`LaunchApplication { id }`, `FocusWindow { id }`, and `CloseWindow { id }`
validate the requested ID against that scaffold state and return
`CommandResult` when the protocol command is accepted. They do not start Linux
processes, inspect real windows, focus a window manager surface, or close a
desktop window.

The matching high-level host wrappers are:

```bash
bridgevm guest-tools list-applications <vm> [--request-id <id>]
bridgevm guest-tools launch-application <vm> --id <application-id> [--request-id <id>]
bridgevm guest-tools list-windows <vm> [--request-id <id>]
bridgevm guest-tools focus-window <vm> --id <window-id> [--request-id <id>]
bridgevm guest-tools close-window <vm> --id <window-id> [--request-id <id>]
```

Application-consistent snapshot freeze/thaw support defaults to a simulated
protocol boundary. The scaffold advertises `fs-freeze` and `fs-thaw` by
default, accepts `FreezeFilesystem { timeout_millis }`, tracks an in-memory
frozen flag, and accepts `ThawFilesystem` only after a successful freeze.
Repeating freeze while frozen returns `filesystem-already-frozen`; thaw without
an active scaffold boundary returns `filesystem-not-frozen`. A successful
`CommandResult` in the default mode means the runner accepted the preflight
boundary message for the current session.

Real Linux filesystem freeze is an explicit opt-in scaffold. Start the runner
with `--real-fsfreeze` and at least one `--fsfreeze-mount <path>` to route
freeze/thaw through the command backend for only those allowed mount points:

```bash
bridgevm-tools-linux \
  --socket <vm-bundle>/metadata/guest-tools.sock \
  --token-file <vm-bundle>/metadata/guest-tools-token.json \
  --real-fsfreeze \
  --fsfreeze-mount /
```

The initial real mode is structured around the Linux `fsfreeze` command
backend rather than a direct ioctl. It may require root or `CAP_SYS_ADMIN`, can
fail on unsupported filesystems or mount states, and must not be presented as
application consistency by itself. BridgeVM does not flush databases, quiesce
applications, coordinate app writes, or otherwise prove OS/app-consistent
snapshot readiness.

The scaffold does not yet apply Linux clipboard, display, shared-folder,
application, window, or network state changes to the guest OS. Freeze/thaw
changes the guest OS only in the explicit real fsfreeze mode described above;
otherwise it remains simulated scaffold state. Drag-and-drop file writes are
available only through the explicit `--file-drop-dir` scaffold output
directory. Those integrations should build on the same newline-delimited
`AgentEnvelope` loop.
