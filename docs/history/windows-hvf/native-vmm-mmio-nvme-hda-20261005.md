# Native VMM MMIO, NVMe and HDA checkpoint — 2026-10-05

This records deterministic development evidence based on
`ea0dd9ea1ce5698e508efa47ebd31df911908b28`. It does not designate a release head
or claim Windows guest behaviour. Capability states and wording remain owned by
`capabilities/windows-hvf.json`.

## Device-load emulation

The native CPU0 and secondary-vCPU paths previously used data-abort access
fields without requiring ISV and wrote device values without applying SSE.
The optional userspace GIC path masked access width but also omitted signed
extension. A shared decoder now requires a valid data-abort syndrome before
device, register or PC effects; read results apply access width, sign extension
and destination width. XZR loads remain discarded and XZR stores remain zero.
Invalid syndromes use the existing fatal-vCPU error paths.

The DXE-entry runner uses the same helper. Other standalone probe decoders are
outside this change. The packaged native runner uses the CPU0/secondary paths.
A helper extracted with the previous native semantics produced three passing
and four failing regressions (exit 101). This was a deterministic extraction,
not an untouched-base or live-guest run. The final helper passes eight tests;
the existing DXE tests pass two. Affected examples compile and all-target
Clippy passes. Independent source review found no required correction.
CPU source commit: `8f97c43c71f10e0ea7e21e5a454f66ab6ce4057b`.
Baseline raw SHA-256: `cbdb9da6c3b130b51e9a5be6f7dbd4fcf0aece64cc82532eb4f3692c632a8482`.
Final helper raw SHA-256: `ff6b9ccf8c5653fefdd3143fbd0e809291bfb058aaeb4cf432227677ce2928f6`.
Final DXE raw SHA-256: `553d6977f7a33d5ffe18b041bbc40f58b090096cc40b811b472e913152beb560`.

## NVMe completion ownership

A full completion queue previously allowed later submissions to overwrite
unread completions and execute additional disk writes. Submission processing
now waits before fetching or executing a command when its completion queue has
no free slot. The pending submission resumes when the guest advances the
completion head. Independent completion queues continue to make progress.
Create-I/O-Completion-Queue rejects a one-entry queue.

Three original regressions failed (exit 101), including overwritten command
IDs and unwanted disk writes. A separate one-entry-queue regression also
failed before the repair. Afterwards the NVMe suite passes 89 tests, with one
existing microbenchmark ignored; four tests cover the new boundary.
All-target Clippy, formatting, budgets and independent review pass.
Integrated source: `91f7c5b93dd2cdb4881b0921a590315e10302c3a`.
The budget append conflict was resolved by preserving both sets of rows.
Baseline raw SHA-256: `d7c957a8dfb1df67c174c765e4ac3530c47deb70d6646e9a78a9c70d244e2d3f`.
Fixed suite raw SHA-256: `ec8183112a0777aeef732009bae891168af627ff96c711f15eaeddc34afdacab`.

## HDA DMA address overflow

After a valid first PCM transfer, rewriting the current descriptor address to
`u64::MAX` caused its addition to the existing offset to panic. The regression
failed against unchanged production code (exit 101). Playback now checks the
addition and follows the existing descriptor-error path: set DESE and clear
RUN before further memory reads, PCM delivery, position or IOC effects.
The surrounding playback algorithm is unchanged; the function is extracted
to keep the structural ceiling from growing.

The HDA suite passes 18 Debug tests, including scratch-buffer reuse; the exact
new regression also passes in Release. Formatting, budgets and independent
review pass. This does not prove a remedy for audible stutter.
Integrated source: `e7e8b08a9ebef96ea61eb0ab640b0747712f8a27`.
The budget append conflict was resolved by preserving both sets of rows.
Baseline receipt SHA-256: `75dc8a7444c7563f6535c33dd7a1458a0d663198ff57fa078a592af55753f87a`.
Focused receipt SHA-256: `72763840d26cf105a5cab5cfcee805e521d5e229e2b2b10be1a9822a77d8636f`.

## Integration boundary

The original failed tests and intermediate formatter/budget failures remain
in the private development records. Temporary test executables were removed.
No guest disk, vars or private Windows media was used for these tests.
Existing structural ceilings never increase; new modules use actual counts.
These repairs implement existing architectural/device contracts and add no
intentional platform deviation.

Initial integrated checks were pending. CI at `fe2e1531` then failed rustfmt
and documentation references: imports needed ordering, and two original local
commits were not published. The successor sorts imports and cites integrated
ancestors. Full successor checks and physical-Mac validation remain pending.
A9, A11 and A19 remain OPEN; ENGINEERING_PREVIEW and known defects are unchanged.
