# Owned bundle import contracts — 2026-10-03

Evidence rank: deterministic tests with generated raw/qcow2 files and actual
CLI/socket commands. No private Windows media or live guest was used.
The focused conclusion is `749976b4653822c2bb171f885364215202652cad`,
integrated as `3e24e6539441ca6311bd36732d57d98c1e164bc3`.
Product truth remains in
[`capabilities/windows-hvf.json`](../../../capabilities/windows-hvf.json).

## Failures and source boundaries retained

At `955c248df28e5d201b930e14338a506f640b3aed`, generated directory and tar
imports each retained the source's selected absolute disk path, failed snapshot
restore after source withdrawal, and retained a real qcow2 backing dependency.
These are six failed portability observations, not a live guest result.

The initial actual release-helper experiment failed both PATH-poison and
recorded-helper refusal cases. Its binary demonstrated invocation behavior,
but the full compiled source inventory was unsealed. It cannot supply sealed
source attribution. A separate replay built from tree
`c53ae96fd9cda2200c428d511f096abdfe8ab4ac` reproduced both failures.
That release binary SHA-256 is
`648698511853a9f317642ac02648dc4f494b2c6502422ad94e6c610a1045b52b`;
unchanged before/after source inventories both hash to
`c3b4e6c8717ea0f02823284c488112fe539e39637b01697416ccee79071a014d`.

The seven-contract PASS at intermediate tree
`74e44ec13965f721f49eb24b9758edc721807dcb` preceded discovery that raw-only
imports unnecessarily required QEMU. A separately sealed debug binary from
that tree failed both directory and tar raw-only cases with QEMU absent.
Its SHA-256 is `b2461b7969d941143a45af0d107c5ba70ed4078523399b03632f1cef354321be`;
matching source inventories hash to
`467f9c28f349e87c4ae7d368b6abbe787acf288180168ff1471e2281067a6a8d`.

Retained failed log identities are below. Draft regressions have their own
source scope; they are not rewritten as failures of the frozen `955c248d` tree.

| Log basename | Recorded result | SHA-256 |
| --- | --- | --- |
| `baseline-955-20261003.log` | Six portability failures | `64a9bf397e8e3967793b073892d5b279c842f12a71521141109246334b549ab6` |
| `storage-first-20261003.log` | Draft storage: 84 passed, 2 failed | `18dcdb854dad277a067d219b17d542e29074933468e20c2938bb8ee6c105d8bf` |
| `storage-second-20261003.log` | Test compile failure: missing `VmManifest` import; no tests ran | `bd92d35e5fdd059b0c2e0dcd8d9e0b92ad5c99647f681ddafcc54238d848eb3b` |
| `storage-fourth-20261003.log` | Draft storage: 90 passed, 1 backing-chain failure | `e1a64d5d1aeb680a84f463390afef9c0c5526d98d8468366eff87c96a02acc22` |
| `fast-rename-before-20261003.log` | One renamed Fast saved-state failure | `d7ce7898d93ec009cb304088ad7eacb4f63fe6600dcb1ab1b35dcba1c5b16c07` |
| `dirty-header-before-20261003.log` | One qcow2-header refusal failure | `6762d8cd3ff82ba7ea634581e0f84afd884a6b955f833f04d721967e5cba0a72` |
| `missing-primary-before-20261003.log` | One missing materialized-primary refusal failure | `03c1b38adb5137ed95d91621d15d565247c6372822286f2fc5d3543aaa4b17e1` |
| `bridgevm-release-helper-before-20261003.log` | Initial unsealed helper: 2 failures | `e4077a5a06b90ba223ac7f3fa5c926f69aadfc4e4141fb36bc1c666fd3b88ad3` |
| `bridgevm-release-helper-c53-before-20261003.log` | Sealed helper replay: 2 failures | `757a7a7dd74b1499e9099e84927d927728a603e0806de28cff1450d0d765ff4c` |
| `bridgevm-import-raw-before-74e4-20261003.log` | Sealed raw-only baseline: 2 failures | `dd7be7522f6a6c256fffe4c9f64379c557990e061a094e87d8a037384648d728` |

An intermediate isolated project check was deliberately stopped with exit 143
after the helper source scope changed. Its log SHA-256 is
`fe8f3e8fcc42610b628b607e422922950d7901faf930cd4a06765d03159cb341`.
It supplies no full-project PASS. The intermediate seven-contract log hashes
to `76b340d2ed9e19644913ab9448f02165eea3768b028fdd2f2ac896914d761c5b`.

## Implementation and final focused proof

Imports copy into exclusive staging, byte-compare copied files, then relocate
typed active-disk, snapshot, disk-create and suspend metadata. Historical
execution evidence remains historical. Renaming rekeys Fast saved-state VM-name
metadata and its filename, preserving saved-state bytes and the manifest UUID.
Internal qcow2 dependencies are validated and rewritten within the
owned copy; external, encrypted, dirty, inconsistent or missing required media
are refused. Publication cannot replace an existing or concurrent destination.
Source files and imported copies remain independent.

Materialized raw-only imports work without QEMU. Release storage helpers use
approved fixed installation paths and reject alternative recorded helpers;
debug builds retain PATH lookup for developer fixtures. The
[Compatibility Mode contract](../../compatibility-mode/README.md) records this
boundary without claiming guest boot or memory restoration.

The final tracked-source inventories before and after the focused checks both
hash to `d185708b3f40a4f950b11c0c396fcc6b41428c9a72661a0ce1cfb1687121bc04`.
They bind focused tree `685077a081a28af4760ed8cbc2422677ce03d1b9`.
The actual debug CLI hashes to
`8b318a7407a7e313d947e4147e5600871a66a315f8a6c39f1432123aeeb8e151`;
the actual release CLI hashes to
`fbeb86b9f81f6b78c0371acd294be2a910b36c6cb25ec9dc60056d0973bba627`.

| Final focused evidence | Result | Log SHA-256 |
| --- | --- | --- |
| Storage | 99 passed, 0 failed | `4b4ad9a21949521698cd4a07a3de1329adf5c58cc4f86276a06957514fd22cff` |
| API | 154 passed, 0 failed | `0ce706ebea13c9fbb82fb39a7290dcc41c2b27172e864f29aee2284f1c7af419` |
| Actual CLI/socket, raw-only and release-helper contracts | 5 + 2 + 2 passed | `4b2c3c10ed1768764b0964b8d6c5e86a4e980091cb26ef04948761a715ba4aab` |

The actual image tools were QEMU 11.0.2. Debug/release Clippy, formatting,
budgets and test reachability passed. New modules use actual-size budgets;
existing structural ceilings were not raised.

Integrated exact-source full-project checks, pushed-SHA hosted CI and the
hosted QEMU 8 matrix remain pending for this tranche and must be recorded in
the change PR. These focused results establish no live guest behavior or
criterion campaign result; no threshold, state or product wording is promoted.
