# Security Model

BridgeVM treats every guest as untrusted.

## Open guest-facing defects (release blockers)

Two firmware-surface defects in the Windows HVF path are known, confirmed in
current code, and tracked as release blockers in
[`capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json). They are
recorded here rather than quietly fixed later because the guest currently
receives a security guarantee that is not being met.

- **A12 — the SMCCC TRNG is not random.** `TRNG_RND_32`/`TRNG_RND_64` are served
  by a deterministic function of the vCPU exit counter, while `TRNG_FEATURES`
  advertises the calls as supported. A guest that seeds a CSPRNG from this
  interface gets predictable output. The fix is to serve the calls only from the
  host OS CSPRNG and to return `NO_ENTROPY` on provider failure rather than any
  fallback value.
- **A13 — PSCI state reporting is nonconformant.** `CPU_ON` returns
  `ALREADY_ON` for a CPU that is only `OnPending`, and `AFFINITY_INFO` ignores
  the requested affinity level while reporting both `OnPending` and invalid
  targets as `OFF`. A conforming guest can therefore read incorrect CPU state.

**Key provenance rule.** Any Windows, vTPM or BitLocker image produced before
the TRNG fix is classified `pre-secure-trng-development-only`. Guest key
ancestry is not observable from the host, so an audit that only sees timestamps
cannot promote such an image. Development images are not deleted, but release
media and its protectors must be regenerated after the fix.

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
