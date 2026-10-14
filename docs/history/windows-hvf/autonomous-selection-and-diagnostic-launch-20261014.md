# Selection, diagnostic launch and readable evidence — 2026-10-14

Evidence rank: **test and static review**, with the failed physical observation retained.
This follows the [fourth development checkpoint](autonomous-readiness-and-sealed-inputs-20261014.md).
The fifth implementation is frozen for validation at `644531995c949f54b097e4dd5b96f224efc6ef8f`.
Combined full-project and exact hosted checks remain pending. A9, A11 and A19 remain OPEN;
no product state, capability wording, sample threshold or known defect is promoted.

## Failed predecessor results remain failures

Exact `5a846f8aeff5d5bb928d563eccbab430a2812e04` later reached a physical UI diagnostic
with development-signed inputs. AMFI refused the signed bare launcher before an AX result.
The same executable bytes verified inside their reconstructed bundle; the bare copy failed
strict validation because its required Info.plist context was absent. The retained result is
observation-refused with cleanup unproven: the independent supervisor receipt is missing.
The queue was still fenced at the 22:18 UTC observation. Process absence does not substitute
for that receipt; later quiescence or quarantine does not rewrite the original cleanup result.
No Accessibility trust, UI action, successful cleanup or repaired native launch is claimed.

Exact `d913596b51df939442a19d60b689ea1ab61227e3` also failed two Cargo assertions in
[manual CI](https://github.com/Ketchio-dev/bridgevm/actions/runs/38644406399) and the
[associated PR CI](https://github.com/Ketchio-dev/bridgevm/actions/runs/38644410486).
The manual job checked out that exact commit; the PR job checked out its synthetic merge.
Both returned eight zero bytes where the PCI boot-media fixtures expected `WINSETUP`:
`pcie_boot_media_reads_from_attached_iso_and_posts_interrupt` and
`platform_reset_preserving_media_and_vars_clears_runtime_state`.
Each cited default-library job reported 1,453 passed, two failed and two ignored, with exit 101.
The local default and Venus selections failed the same two methods, with 1,453 and 1,477
passed. The full check finished at 22:19:16 UTC: 43 stages passed and two failed out of 45;
all 5,091 tracked files stayed unchanged. The four shim selections passed 425 / 1,404 / 65 /
379 methods, with three existing Control skips. These passes do not erase the Rust failures.

## Clone completion respects a later selection

Integrated [aabe9e1b06a8](https://github.com/Ketchio-dev/bridgevm/commit/aabe9e1b06a8ff56e477358c13f70e1c4068b548)
captures the selected identifier at clone admission and compares it before reloading.
Ordinary completion still reveals the copy when selection is unchanged. A later VM or import
selection remains selected; move and transfer behavior are unchanged.
The four new methods initially passed two and failed two, with four assertion failures.
Fixed native and actual shim selections each passed 16 methods, including unchanged clone
lifetime/admission controls. Equality concerns the current ID: A → B → A still reveals the
copy, and ordinary reload may reconcile a removed registration. The transfer is injected and
publishes tiny invented metadata; no clone helper, rendered GUI or guest execution is proved.
An initial obsolete baseline-bundle lookup failed; the real bundle was retained before rebuild.

## Source-build guidance does not promise an untested compiler

Integrated [d978f2fba1af](https://github.com/Ketchio-dev/bridgevm/commit/d978f2fba1af9e96d5c1a226b1546106f15cfd9b)
removes the unsupported Swift 5.9+ promise from the landing page, README and install guide.
CONTRIBUTING identifies the unpinned runner-provided Swift used by the macOS app-build job.
Current source uses `nonisolated(unsafe)`, introduced in [Swift 5.10](https://www.swift.org/blog/swift-5.10-released/); this does not establish
whole-app compatibility with 5.10. The oldest complete-app toolchain remains unvalidated,
and the unchanged package tools-version declaration is explicitly acknowledged.
Neither installer script has a Swift minimum admission check. No old compiler was executed.
The landing contract, documentation checks and budgets passed. Actual desktop/mobile build
sections reflowed without horizontal overflow; the original design, logo and caveats remain.

## Diagnostic supervision uses the verified bundle executable

Integrated [1492571cc818](https://github.com/Ketchio-dev/bridgevm/commit/1492571cc8184606c0f04247c31eeb43d0462fd8)
selects the already reconstructed, signature-verified driver bundle executable for both
existing diagnostic routes. Canonical regular-file and sealed-digest checks precede launch;
there is no fallback to the bare transport copy or an alternate executable.
The first five-method baseline had four failures and two shared-fixture errors. Isolating
fixture variants retained six failure records with no errors; the same five methods then
passed after repair. Seven final new methods plus related controls passed 49 in total.
Signature and process-launch calls are substituted in these tests. Both actual preparation
routes are reached, but final UI failure is deliberately retained. Cancellation bounds,
child termination and independent two-role cleanup requirements are unchanged.
Actual signed execution, distinct LaunchServices ownership and cleanup still need live proof.

## Four hosted workflows retain four independent jobs

Integrated [736c2fdf0f19](https://github.com/Ketchio-dev/bridgevm/commit/736c2fdf0f19373f808f315fdca3d4722dde1c28)
combines four Ubuntu Python contract workflow files into one explicit matrix. Push and PR
still expand to four isolated jobs each, with the same names, commands, checkout pin, runner,
five-minute timeout and read permission. `fail-fast: false` preserves peer-job independence.
All 27 existing Python methods and 11 expansion refusal controls passed; workflow YAML and
budgets passed. Workflow/run grouping changes; no runtime or cost reduction is claimed.
Static equivalence and local checks do not prove successor hosted scheduling or pass status.

## A11 history is preserved rather than repeatedly expanded

Integrated [7fce633401bb](https://github.com/Ketchio-dev/bridgevm/commit/7fce633401bb2c421bbf68d0f04a0f99db0224e9)
replaces the current A11 measurement with a dated 602-byte summary and preserves all 226,311
original decoded UTF-8 bytes in the [historical archive](../../archive/a11-measured-through-20261014.md).
The payload SHA-256 remains `2b3c8ed0abff91fc3f1e8230827dd690ce1a9a4c12de1f3a4b4049cbaca02a91`.
All 157 prior evidence paths stay in order; only the archive path is appended. At that individual
integration, every other registry field, README, STATUS and prior observation archive is
unchanged. The generated matrix changes only by replacing that measurement; prior catalog
rows remain ordered. This does not erase the earlier onboarding edit to README.
A path-specific `-text` attribute preserves checkout bytes; new 19/3-line files use actual
ceilings, with no existing increase. Copying history does not revalidate its observations.
The original JSON-checker usage failure is retained; corrected syntax checking is not full
JSONSchema validation. After local author commit the normal registry check passed its four
smoke tests, then refused `.gitattributes` freshness. The tested commit was not advanced.
Root's first integration preflight used the wrong evidence key and refused before any write;
the corrected preflight used `evidence_paths`. That failed attempt also remains recorded.

## PCI boot-media fixture follow-up

Integrated [644531995c94](https://github.com/Ketchio-dev/bridgevm/commit/644531995c949f54b097e4dd5b96f224efc6ef8f)
repairs test setup only. Both fixtures made a queue ready but notified with status zero;
the unchanged modern guard correctly refused before `DRIVER_OK`. Shared setup now writes
status `0x0f` after queue readiness, retaining every queue-write acknowledgement and the
original data, interrupt and reset assertions. Each unchanged baseline method failed once;
each fixed method passed. Default library tests passed 1,455 and Venus passed 1,479, with
zero failures and the same two ignored controls in each. Existing modern-ready,
modern-not-ready and legacy-before-DRIVER_OK controls passed unchanged. The fixtures model
PCI BAR MMIO aliases; they do not prove complete PCI initialization or guest boot.
Production behavior, the readiness guard and the predecessor failures remain unchanged.

## Evidence bindings

This table omits private paths; external reports retain author mappings and failed iterations.
No new live result, private Windows prerequisite or clean-Mac journey is established here.

| Retained record | SHA-256 |
| --- | --- |
| Clone selection report | `3eb1852c6114d49558418e5feefb3ba974088cb3e25913da8b9d90d441e4ec3e` |
| Clone selection independent review | `7e535970abc2f5ddea934fa7f04ca567832f75793c5073635915f89e9b2471b1` |
| Onboarding report | `6b9f6e87ef222ad19adfac7b661a9f371d48095221bcfb884185146d4d2f74e1` |
| Onboarding independent review | `ad407ff81a7b757ef4d6358fc77fb8e3ccdcc6e32ddd0bbafc83b0e19e6e8fdb` |
| Bundle supervisor report | `aa09ca5ed73a426a127573e8045e23b6593186ba5510f87f6f315f36a4b35885` |
| Bundle supervisor independent review | `b65c2862a9831fbd75d1426f199ef7fd09afde8ff04892c75a409746108c79a4` |
| Four-workflow matrix report | `eb7465e0b66b6de47206f16edb50e1bcb021dafb4dfd4f22eec1a3e1185f833b` |
| Four-workflow matrix independent review | `8aa85f971547518598768e59a52ab71bdd214deb2673139c625e63ef7db3b04c` |
| A11 exact archive author receipt | `89825ec39035c0fecc83c3e6e02f8e3a20e0b19cbd84bfe03c56a57e84cc3868` |
| A11 independent commit binding | `851e0eed5634f9977829e55f91db79f8b232f860aededf3a390e543cd968badb` |
| Exact5a failed launch audit | `d55f960ff8449a06050f727be6594054a5931fcb5b0eaa8bb3b2a84a8b39aef8` |
| d913 hosted failure classification | `4bbf9192eaa5bdaadc438939b45b67681037c03343b021506755757701e3848c` |
| PCI fixture report | `18ada425c83f2fcfa71500ba5876f4b3377a2571b607cd649421cf1cc68bc2e4` |
| PCI fixture independent commit review | `d54b45c5c6f8ece035847cbfbdd65ba7120f7c3ab538386858bd0832d51941ea` |
| Fourth final local result | `922b29f2441cc204dc248a2eb225768e713807f7ba6c3f8fefcffabf052ae465` |
| A11 refused integration preflight | `7250397e517491477bfd370ef7fb295d5a37705f70dbd9a4f72729f6d8d41ed2` |
| Fifth combined source review | `1af3a62b83542389f0f60c98af8bac58f287a70ac1155377645562620fbdd1b7` |
