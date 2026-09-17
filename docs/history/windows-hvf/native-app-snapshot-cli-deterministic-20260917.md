# Native app powered-off snapshot CLI — 2026-09-17

Classification: historical deterministic evidence. Product state and current
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A11 and A19 stay OPEN.

## Implemented boundary

Source `fd2157e11d43d36fa98ef0c4131256c6feca25f4` adds
`bridgevm app snapshot-create ID` and `bridgevm app snapshot-restore ID` for a
saved own-HVF Windows VM. Both commands resolve the disk, UEFI vars and snapshot
location from the exact native library entry and reuse the existing atomic
managed-pair helper. Create performs a separate post-create verification.

The parser accepts one exact VM ID and no media or secret arguments. Pending
installation and unsupported backends are refused before helper dispatch. The
existing media lease makes a running VM fail closed. Output uses
`bridgevm.app-snapshot.v1` and states that a completed host operation does not
prove a Windows boot or guest-visible restore.

## Deterministic checks

- the Rust CLI suite passed 150 unit tests and all real-process integrations,
  including exact Unicode-ID forwarding and invalid-argument refusal;
- three focused native snapshot CLI tests passed, as did the existing snapshot
  command suite and installation CLI regression selection;
- 68 focused snapshot-pair tests passed;
- formatting, strict CLI clippy, diff hygiene and structural budgets passed;
- the full project check at the exact source commit completed every executable,
  app, security, documentation and structural step, including the default and
  Venus Rust suites, 381 probe tests, and macOS shim suites 425, 808 with two
  required live-only skips, and 62. It correctly failed only capability-registry
  freshness before this checkpoint was recorded.

## Evidence limit

The helper fixture proves command selection, saved-path binding, create/verify
ordering and refusal paths. It does not boot Windows, observe a guest marker,
interrupt a real create or restore, or add a live sample. It therefore advances
the A19 product-lifecycle path without closing A19. Hosted exact-head CI remains
required before merge.
