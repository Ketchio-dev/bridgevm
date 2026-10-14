# Interrupt and fixture follow-up — 2026-10-14

Evidence rank: **test**. Companion to
[the transfer preservation checkpoint](autonomous-import-preservation-20261014.md).
Focused results below do not replace integrated local or exact-SHA hosted checks.
Earlier local and hosted failures retain their failed status. No live sample
count, guest capability, criterion or release eligibility is promoted.

## Delivering an existing xHCI event after interrupt enablement

An ordinary command completion could leave an event TRB and IMAN.IP pending but
never notify after IMAN.IE or PCI MSI-X was enabled: platform flush drained only
PCI pending-bit-array (PBA) messages, while disabled delivery had armed none.
Flush now retries the existing pending-enabled source path after draining PBA
and clearing delivered source flags. This order prevents duplicate delivery;
function/vector masks, explicit acknowledgment and immediate delivery remain.

The final unchanged seven-case public-platform baseline passed three and failed
four, then all seven passed. The 207 related xHCI library cases also passed.
Synthetic guest RAM contains a checked Enable Slot event before notification
assertions. Later enable delivers once without a new command; another command
rearms, masked notifications replay, and acknowledged events remain quiet.

Earlier six-case baselines passed three and failed three and remain recorded.
Fixtures were strengthened to initialize rings before Run, set DCBAA/IMOD0 and
exercise function masking. Review also corrected the reset control to halt and
observe HCH before HCRST; final paired baseline and fixed runs were repeated.
An initial reviewer objection to the historical specification citation was
retracted after reading its full section. No IP/EHB timing defect is claimed.
Reset coverage starts with a disabled source lacking an armed PBA request; it
does not prove reset clears arbitrary PCI pending state.

## Respecting the xHCI global interrupt-enable gate

With Run set and global USBCMD.INTE clear, a command completion still emitted
MSI-X or armed PBA. New source requests now require INTE; event completion/IP
remain pending for later enable. Already-armed PCI PBA replay remains governed
by PCI masks, so clearing INTE does not discard an earlier accepted request.
The repair adds no Run gate and preserves IMAN.IE, W1C and reset handling.

The unchanged final seven-case baseline passed three and failed four; all seven
passed after repair. Combined new and preceding late-enable tests passed 14/0.
The first related-library run passed 200 and failed seven: those notification
fixtures had omitted INTE. Only explicit enable setup was corrected, preserving
all assertions; the final library selection passed 207/0. The earlier six-case
baseline (two pass, four fail) and all failed runs remain recorded.

Both interrupt repairs are deterministic controller/platform message evidence.
They prove neither physical MSI delivery nor guest behavior. EHB/ERDP handling,
moderation, timing and complete xHCI compliance remain outside these conclusions.
No deliberate new guest-contract deviation is introduced.

## Canonical expected paths in CI fixtures

Three corrections preserve production behavior and compare the intended identity
or reach the intended injected boundary under aliased temporary directories.

| Fixture | Retained baseline | Corrected focused proof |
| --- | --- | --- |
| Rust literal-name import | Same frozen binary: physical 8/0, alias 0/8; freshly extracted alias fixture 0/8 | Fresh alias 8/0 and physical 8/0 |
| Swift warning details | Unmodified native selection: physical 3/0, system alias 1/2 | Native 11/0 and actual shim 11/0/0 in both venues |
| Retained Cancel | Preserved native and shim binaries: physical 2/0, alias 0/2 | Fresh native 2/0 and actual shim 2/0/0 in both venues |

Rust literal-name expectations now canonicalize the selected disk path, matching
import metadata. Filename/content, literal-versus-nested names, receipt uniqueness,
source/archive preservation and existence assertions remain. A six-line helper
keeps the structural ratchet. The exact earlier full run failed before this
integration test executed; its failure is established by separate paired replays.

Warning details use POSIX realpath for expected paths because native Foundation
resolution retained the system alias while directory enumeration returned its
physical path. Actual scanner outputs and exact path-set equality are unchanged.
The retained baseline binary was later overwritten by its fixed rebuild; saved
source copies, raw logs and historical hash remain, not rehashable baseline bytes.
The 11-method final selection is identical between native and actual shim.

Retained Cancel canonicalizes its owned existing temporary parent before making
the synthetic cache. An exact-source diagnostic found the existing unsafe-source-
lock-path refusal before any injected process, preparation or finalization call.
The earlier paired binaries came from the preceding author checkpoint with nine
relevant source hashes equal to the candidate; they are not whole-candidate builds.
Fresh corrected runs use the candidate plus two fixture lines. Cancellation
conditions, source-lock policy and five-second gate deadline are unchanged;
temporary diagnostic prints were removed. These unshown synthetic UI tests prove
no physical click, rendered window, real installer, key operation or guest run.

## Finite TPM socket paths and execution venue

The failed prior full run recorded normal-library 1435/2 with two ignored cases,
feature-library 1459/2 with two ignored, and control-shim 1359/2 with three skipped.
Its TPM failures occurred at synthetic UnixListener pathname validation before
backend construction/reset. The control failures were the separate retained-
Cancel fixture issue above; no single cause explains the entire failed check.

The exact preserved normal and feature binaries replayed two TPM methods each:
four long-path invocations failed and four short-path invocations passed. Source
and executable hashes stayed unchanged. The two venues were distinct owned
directories on the same existing APFS filesystem, not the same directory inode.
A shorter owned TMPDIR spelling addresses this observed fixture limit without
changing TPM source, test expectations, mounted storage or gate requirements.

An initial explanatory Rust 1.93 reference was corrected to the actual 1.97
build provenance. Independent review then read that installed commit's source;
no reproducible standard-library build or exact pathname-limit boundary test is
claimed. These eight invocations are paired synthetic tests, not statistical
samples, actual swtpm/keys, or live TPM evidence. The original full check stays
failed; a subsequent complete local and hosted check needs its own receipt.

## Bound evidence

Independent reviews bind source/artifact hashes and retained experiments.
Repeated suites above overlap and must not be added as unique test counts.
The TPM row identifies an evidence report because no source patch was needed.
No private paths, guest payloads or capability edits are included.

| Conclusion | Patch or report SHA-256 | Independent review SHA-256 |
| --- | --- | --- |
| xHCI late enable, final v2 | `48307d0f23d7b0fe29e3f6d25db2b5ebcf93161c5ea5ecf804b439ebff562e7e` | `75a5a8d51d011f19a11394a3a1a7607c604d8b4e32b7135a70963e531c341118` |
| xHCI INTE | `51520a7dae1d650781b701c448946bab98c36991426ef47e43d244ceafc3ee16` | `c51e6a092d19b1834a142f81f5e25beb55f654099118c3e52eaed923f8a41e38` |
| Rust literal paths | `6afdb639d9bd30d8abad5186928895df3ede744c5bc6bfb060cea8a05e134bd9` | `332cc095b66e74fb1ce79bf5fcc20dcf705bcdd7759e0ffd4a3fe79bcebf1b39` |
| Swift warning paths | `d77f4bfa852e9aef7a53bb47fbd05be9fb1bc9c01f18e07122e00fa2c44c64ef` | `4777bd82f4a22a4dbc3b30f91663c718ec23086f084252a30e8d1dd8d0cd652d` |
| Retained Cancel paths | `49bdc005ad4eca3190eb8eb90e1223448b5351edc022f9126ed9ea04f06e58b3` | `cced0c73b9a383d31666165d3a68766ba6fbcba1c0b75013790b478c659755b6` |
| TPM venue report | `a7453abc10f4f635196057ebc35d763dd1d95322d7d1ec7dd63ebf571ac42859` | `3af3d72601a600ce51e3130014372de9387e97edc666b2fb2ae9a0324d2da2ca` |
