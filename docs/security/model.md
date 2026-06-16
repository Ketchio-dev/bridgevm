# Security Model

BridgeVM treats every guest as untrusted.

Default rules:

- Clipboard sync is allowed but should support direction controls and expiry.
- Shared folders require user-approved paths.
- Guest-to-host command execution is disabled.
- Host-to-guest command execution is disabled by default.
- Agent updates must be signed before they are accepted.

Every VM bundle has its own manifest and guest-tools channels use a per-VM
tools auth token. `bridgevm-agentd` turns an untrusted `GuestHello` envelope
into a validated agent session only after the token matches VM policy and the
advertised feature capabilities are allowed. It also keeps request/response
plumbing honest by accepting `CommandResult` only for pending host command
request IDs.

The Linux tools scaffold prefers `--token-file` over `--token` so the token is
not exposed through process arguments during normal smoke tests. It rejects
empty token input before sending `GuestHello`. `bridgevm guest-tools
linux-command <vm>` should be used for scaffold smoke tests when possible: it
generates a manifest-compatible `bridgevm-tools-linux` invocation for the device
or development socket transport, points at the per-VM token metadata file, and
expands policy-derived capabilities without asking users to manually copy
capability lists. Full guest token provisioning is still an installation
problem: the token file must be delivered to the guest with permissions
appropriate for the future service account, and QEMU command lines must continue
to avoid containing the token value.

`bridgevm diagnostics bundle <vm> --output <dir>` follows the same assumption
that support artifacts may leave the local trust boundary. The diagnostics
bundle collects `manifest.yaml`, `logs/`, and `metadata/`, then writes
`diagnostic-bundle.json`, but excludes disks, installer or restore media,
sockets, and lock files. Bundle JSON is redacted before it is written: the
guest-tools token is removed, and sensitive JSON keys are replaced with redacted
values instead of being copied into the bundle. URL query strings in JSON
metadata are also replaced with a redacted marker so signed download URLs do
not leak through support artifacts.
