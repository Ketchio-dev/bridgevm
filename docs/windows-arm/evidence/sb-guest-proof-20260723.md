# Windows ARM64 Secure Boot + TPM2 measured-boot guest proof (2026-07-23)

## Result

`SEC-SB-MEASURED` is live-proven on the BridgeVM-owned HVF engine. One Windows
11 ARM64 Home boot reached the resident guest agent with Secure Boot enabled,
a ready TPM 2.0 device, a fresh non-empty Windows measured-boot log, and
coherent host-side TPM PCR traffic. The guest shut down through PSCI and the
writable NVMe image was flushed.

This receipt does not claim BitLocker. The installed edition is `Core`
(Windows 11 Home), and no Pro upgrade key was available.

## Live inputs

- Disk: `~/BridgeVM/work/wall-run-20260723.raw`
- Vars: `~/BridgeVM/work/wall-run-20260723-vars.fd`
- Firmware: `crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd`
  - SHA-256: `b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b`
- Secure Boot object source: Microsoft `secureboot_objects` v1.6.5, commit
  `798cdc513e0c192fe90e99637105748ed3bb4ca5`
- Official ARM64 asset: `edk2-aarch64-secureboot-binaries.zip`
  - SHA-256: `8c87c63e8ba0385d17238e8feb3b87de25007bec8e43251246bccbf18007af20`
- Policy: official `LegacyFirmwareDefaults` Windows transition set:
  - `dbx`: `329f9ec34a8ae3c9e7eddaeba82a84f598c44853790394314dd88b563c667e1a`
  - `db`: `584ff437815864a48a2e4c1cc13af8bc19471b140c8085e9de7c738354a91fdc`
  - `KEK`: `cc3a5dbc7b3aec3b60c0da33510bf93f402479bbf445dc360e6111afa70c6342`
  - `PK`: `485aca0cb5f875572c905e6f19ec0a249cf438b005a3e27257ac4bd3f56777bd`
- Evidence directory: `~/BridgeVM/runs/wall-c2-transition-proof-20260723/`

The transition `db` contains both Microsoft Windows Production PCA 2011 and
Windows UEFI CA 2023. This was required because the installed 25H2 boot manager
is still signed through Production PCA 2011. The previous 2023-only policy
correctly rejected it with `Access Denied -- rejected probably by Secure Boot`.
Microsoft documents the 2011 CA as an allowed compatibility bridge for legacy
or not-yet-updated systems while 25H2+ moves to the 2023 CA.

Official references:

- https://github.com/microsoft/secureboot_objects/tree/v1.6.5
- https://github.com/microsoft/secureboot_objects/blob/v1.6.5/Templates/LegacyFirmwareDefaults.toml
- https://learn.microsoft.com/windows-hardware/manufacture/desktop/windows-secure-boot-key-creation-and-management-guidance

## Provisioner correctness repair

The first live attempt found a real varstore corruption bug. EDK2
`VARIABLE_STORE_HEADER` is 28 bytes (`GUID + Size + Format + State + Reserved +
Reserved1`), but BridgeVM began variables at `base + 24`, overwriting
`Reserved1`. The provisioner, boot seeder, walkers, and fixtures now use
`base + 28`, and the smoke requires the complete header to remain byte-identical.

The repaired store preserves the original header and existing boot variables,
then appends `dbx`, `db`, `KEK`, and `PK` in that order with authenticated
attributes `0x27`; PK remains last. The deterministic provisioning smoke passes.

## Guest facts

Captured through the resident `bvagent` channel in
`c2-guest-output.txt`:

```text
EditionID=Core
ConfirmSecureBoot=True
SetupMode=0 Bytes=00
SecureBoot=1 Bytes=01
TpmPresent=True
TpmReady=True
TpmEnabled=True
TpmActivated=True
TpmOwned=True
SpecVersion=2.0, 0, 1.83
```

The same boot emitted Kernel-Boot event 273 saying the BCD `testsigning` option
was not applied because Secure Boot was enabled, an independent enforcement
receipt.

A diagnostic command briefly printed TPM `OwnerAuth`; it was immediately
redacted from the retained logs. No credential value is included in this
receipt. Before production use, this development vTPM identity should be reset
or reprovisioned.

## Measured-boot receipt

The latest guest log was:

```text
C:\Windows\Logs\MeasuredBoot\0000000006-0000000000.log
size=62382
sha256=27EB9AD92617166AFCD15DC411407F276A91EBACD60801521041999747166B0E
```

The agent returned the complete file over its bounded chunked protocol:

```text
bytes=62382 expected=62382 chunks=3/3 ok=true
```

The final host TIS summary was:

```text
commands=1134 success=1078 errors=56 backend_failures=0
malformed_commands=0 malformed_responses=0
startup=1 self_test=1 get_capability=258
pcr_read=431 pcr_extend=82
```

This satisfies the PCR 7 evidence boundary through correlated Windows
measured-boot output plus host-observed TPM `PCR_Read`/`PCR_Extend` traffic;
the current trace aggregates PCR command counts rather than decoding individual
selection bitmaps.

## Shutdown and persistence

```text
stop: PSCI 0x84000008 (system off)
NVMe disk written back: ...wall-run-20260723.raw
agent-service-gate status=0
```

No BitLocker claim is made: Windows Home is the explicit edition blocker, not a
BridgeVM TPM or Secure Boot defect.
