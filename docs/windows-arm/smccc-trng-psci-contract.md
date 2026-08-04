# SMCCC TRNG and PSCI firmware contracts

Document status: **Current**
Last reviewed: **2026-08-04**

These are the two firmware interfaces a Windows or Linux guest trusts for
randomness and CPU lifecycle. Both had defects found by external review of the
probe's HVC dispatch. This page pins the authoritative behaviour so the
implementation can be checked against a specification rather than against
whatever the guest happened to tolerate.

## Authorities

| Interface | Specification |
| --- | --- |
| TRNG | Arm DEN 0098, *Arm True Random Number Generator Firmware Interface*, version 1.0 |
| PSCI | Arm DEN 0022, *Power State Coordination Interface*, version 1.1 |

Where this page and a specification disagree, the specification wins and this
page is the bug.

## TRNG — implemented (A12)

Served by `bridgevm_hvf::smccc_trng` over the host CSPRNG in
`bridgevm_hvf::host_entropy`. The primary and secondary vCPU loops share one
dispatch in `trng_dispatch.rs`.

### Function IDs

| Function | ID |
| --- | --- |
| `TRNG_VERSION` | `0x8400_0050` |
| `TRNG_FEATURES` | `0x8400_0051` |
| `TRNG_GET_UUID` | `0x8400_0052` |
| `TRNG_RND32` | `0x8400_0053` |
| `TRNG_RND64` | `0xc400_0053` |

### Status values

These are TRNG-specific and deliberately separate from the PSCI namespace.

| Status | Value |
| --- | --- |
| `SUCCESS` | `0` |
| `NOT_SUPPORTED` | `-1` |
| `INVALID_PARAMETER` | `-2` |
| `NO_ENTROPY` | `-3` |

### Rules

- `TRNG_VERSION` returns `0x1_0000` (major 1, minor 0).
- `TRNG_FEATURES` returns `SUCCESS` **only** for implemented function IDs and
  `NOT_SUPPORTED` otherwise.
- `RND32` accepts at most 96 bits, `RND64` at most 192. A larger request is
  `INVALID_PARAMETER`.
- A zero-bit request succeeds and draws no entropy.
- Entropy fills **X3 first**, then X2, then X1. Bits above the requested count
  and any unused register read as zero.
- Entropy comes only from `SecRandomCopyBytes(kSecRandomDefault, ...)`. A
  provider failure returns `NO_ENTROPY` with zeroed data registers; a platform
  with no provider returns `NOT_SUPPORTED`.
- **There is no fallback.** Exit counts, timestamps, PIDs and constants are not
  entropy sources.
- `PSCI_FEATURES` does not answer for TRNG functions.

### The defect this replaced

The previous handler returned
`exits * 0x9E37_79B9_7F4A_7C15 + 0xD1B5_4A32` with rotations for the other two
registers, where `exits` is the vCPU exit counter. A guest can influence and
approximate its own exit count, so the output was predictable while
`TRNG_FEATURES` advertised the service as supported. `TRNG_FEATURES` also
returned success for every query, including unimplemented functions.

Any Windows, vTPM or BitLocker image created before this fix is classified
`pre-secure-trng-development-only`. Guest key ancestry is not observable from
the host, so release media must be regenerated rather than audited.

## PSCI — implemented (A13)

The state logic lives in `bridgevm_hvf::psci` as pure functions over the
topology, and `psci_adapter.rs` supplies the probe's locking and tracing. Both
vCPU run loops call the same adapter.

### `CPU_ON` state table

| Current state | Result |
| --- | --- |
| `Off` | `SUCCESS`, transition to `OnPending` |
| `OnPending` | `ON_PENDING` |
| `On` | `ALREADY_ON` |
| Unknown MPIDR | `INVALID_PARAMS` |

The state check and the `Off -> OnPending` transition must be atomic, so two
racing callers cannot both receive `SUCCESS`.

### `AFFINITY_INFO`

- Reads the affinity level from **X2**; levels 0–3 are valid and anything else
  is `INVALID_PARAMS`.
- Masks the target MPIDR to the requested level and searches the whole
  topology, including CPU0.
- Returns `ON` if any matching CPU is `On`; otherwise `ON_PENDING` if any is
  `OnPending`; otherwise `OFF`.
- A target matching no CPU is `INVALID_PARAMS`, not `OFF`.

### The defects this replaced

- `CPU_ON` returned `ALREADY_ON` for a CPU that was only `OnPending`, so a
  caller was told a CPU was executing before it had taken an instruction.
- `AFFINITY_INFO` ignored X2 entirely, so a cluster-level query was answered as
  if it named one CPU.
- `AFFINITY_INFO` collapsed `OnPending` into `OFF`, making a starting CPU
  indistinguishable from a powered-down one.
- `AFFINITY_INFO` answered CPU0 as `ON` without consulting any state, and
  reported an unknown target as `OFF` rather than `INVALID_PARAMS`.
- `PSCI_FEATURES` also answered for the five TRNG function IDs.

### Relationship to the A1 boot stall

Failing A1 boots never issue `CPU_ON` at all: the SMP trace recorded zero
`Off -> OnPending` transitions in every failing reboot generation. These PSCI
defects are therefore **not** demonstrated to cause the stall. They were fixed
because they are wrong against the specification, not because a fix is
predicted to close A1, and no A1 claim rests on them.
