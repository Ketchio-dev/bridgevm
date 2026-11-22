# Security Model

BridgeVM treats every guest as untrusted.

## What the environment may decide

A release build must run only what the signed bundle contains. Four environment
overrides would otherwise let something outside it choose the executable or the
repository the app reads from, so they exist in debug builds only:

| Override | What it would let an outside party choose |
|---|---|
| `BRIDGEVM_REPO_ROOT` | Which checkout the app reads scripts and assets from. |
| `BRIDGEVM_SWTPM_BIN` | Which binary backs the guest's vTPM. |
| `/opt/homebrew/bin/swtpm`, `/usr/local/bin/swtpm` | The same, via a PATH-shaped fallback. |

This is enforced by absence rather than by a runtime check: a string that is not
in the release object cannot be read by any code path there.
[`scripts/check-release-overrides.sh`](../../scripts/check-release-overrides.sh)
proves it, and refuses to pass when a name is missing from the debug build too,
since that would make the comparison vacuous. It runs in the `swift` CI job
against a real release build.

## Guest-facing firmware contracts

Two firmware-surface defects in the Windows HVF path were found by external
review of the probe's HVC dispatch. The full contracts are in
[the TRNG and PSCI contract page](../windows-arm/smccc-trng-psci-contract.md);
state is tracked in
[`capabilities/windows-hvf.json`](../../capabilities/windows-hvf.json).

- **A12 — SMCCC TRNG. Fixed.** `TRNG_RND32`/`TRNG_RND64` were served by a
  deterministic function of the vCPU exit counter while `TRNG_FEATURES`
  advertised the calls as supported, so a guest seeding a CSPRNG from this
  interface received predictable output. Entropy now comes only from
  `SecRandomCopyBytes`, with no fallback: provider failure returns `NO_ENTROPY`
  and a platform without a provider returns `NOT_SUPPORTED`. `TRNG_FEATURES`
  reports only implemented functions, and `PSCI_FEATURES` no longer answers for
  the TRNG namespace.
- **A13 — PSCI state reporting. Fixed.** `CPU_ON` reported `ALREADY_ON` for a
  CPU that was only `OnPending`, and `AFFINITY_INFO` ignored the requested
  affinity level while collapsing both `OnPending` and unknown targets into
  `OFF`, so a conforming guest could read incorrect CPU state. The state table,
  affinity levels and error codes now follow DEN 0022, and the state check and
  transition share one critical section.

**Key provenance rule.** Any Windows, vTPM or BitLocker image produced before
the TRNG fix is classified `pre-secure-trng-development-only`. Guest key
ancestry is not observable from the host, so an audit that only sees timestamps
cannot promote such an image. Development images are not deleted, but release
media and its protectors must be regenerated.

Default rules:

- Clipboard sync is allowed but should support direction controls and expiry.
- Shared folders require user-approved paths.
- Guest-to-host command execution is disabled.
- Host-to-guest command execution is disabled by default.
- Agent updates must be signed before they are accepted.
