# Native VMM cancellation, SGI targeting and queue geometry — 2026-10-06

These are deterministic repairs to the existing VMM contracts, with no new
live Windows, release or capability claim. Runtime integration began at
`4e93ba5c79cead9a4b8387338cb3324eedf8ed59`; the subsequent platform fixture
repair is recorded below. Exact combined project and hosted results remain
pending at this source checkpoint.

## Published completion wins over watchdog cancellation

The old watchdog checked completion before each one-millisecond wait, then
unconditionally requested cancellation after the last wait. Completion
published during that last wait therefore still produced a cancellation.
The extracted production decision now arbitrates completion against timeout
with compare-exchange. The entry probe uses the same 100 ms helper.

Five unchanged controlled fixtures give **3 PASS / 2 FAIL** on the original
loop, then **5 / 0** in Debug and Release. They inject waiting and cancellation
into the actual production decision helper; they do not invoke HVF. Broader
watchdog-lane library runs each pass **1,295 / 0**, with one existing ignore.
Fixture SHA-256: `11f5bbf65145606f4fb74d6061607ef3bbc905a088ba2b1266978bcc141204bd`.

The return-before-publication and timeout-claim-before-provider-call windows
remain. A timeout winner may still request cancellation after physical execution
returns. Joining precedes helper return, re-entry and destruction; this is not
proof that every stale hardware cancellation is eliminated. The initial Clippy
failure for an unused Duration import is retained; removing that import and
the final checks pass. No real scheduler or guest-stall conclusion follows.

## Targeted SGIs respect Aff2

The optional userspace GIC matched Aff1 and the target list but ignored Aff2.
Guest operand `0x0000000103000002` could thus deliver SGI3 to CPU1 despite
targeting absent Aff2=1. Selection now compares the modeled MPIDR Aff2 too.
[Arm IHI 0069G](https://documentation-service.arm.com/static/601412d54ccc190e5e681269)
§12.2.21 defines targeted affinity matching and ignoring absent PE targets.

Eight unchanged public sysreg/MMIO fixtures plus 63 existing cases give
**67 PASS / 4 FAIL**, then **71 / 0** in Debug and Release. They cover absent
Aff2, sixteen-CPU fanout, pending behind active/disabled SGIs, valid targeted
delivery, empty targets, existing Aff1 rejection and all-but-self broadcast.
Fixture SHA-256: `0926a5f76e8f3a14c2c47f7a0fb8ea3e0ad695cb4b34809f422af2b5572e6098`.

The initial fixture failed compilation due to ambiguous integer conversion;
no tests executed then. Explicit u64 conversions produced the frozen pair.
Default Apple-GIC behavior, unsupported Aff3/RSS inputs, SPI routing, priority
register defects and live workload impact are outside this result.

## Validate I/O queue geometry before installing an NVMe queue

The old controller accepted a one-slot SQ and PC=0 SQ despite advertising
CAP.CQR=1. Both could install unusable or incorrectly interpreted queues.
A shared CQ/SQ validator requires 2 through CAP.MQES+1 entries and PC=1;
invalid sizes return command-specific Invalid Queue Size (0x0102).
[NVMe 1.4](https://nvmexpress.org/wp-content/uploads/NVM-Express-1_4-2019.06.10-Ratified.pdf)
§4.1.3 and §§5.3–5.4 define these sizes and statuses; Figure 155 specifies
Invalid Field in Command for PC=0 when contiguous queues are required.

Eight unchanged valid-setup admin/MMIO fixtures give **3 PASS / 5 FAIL**, then
**8 / 0** in both profiles. Two failures were incorrect queue installation;
three were incorrect size-status codes. Rejection preserves the queue ID for
a valid retry. Buffered disk READ/WRITE, CQ phase, minimum-depth wrapping and
the advertised 1,024-entry maximum remain covered.
Fixture SHA-256: `61657b955530bccd113a81f7f35670d1956adadae2a9d7f8eb923c280cc7ab99`.

The first broader NVMe run was **178 PASS / 4 FAIL / 1 ignored**: three old
backpressure fixtures used PC=0 and one expected the old CQ size error. Those
setups and normative status expectations were corrected; disk/backpressure
assertions remain. Module Debug/Release then each passed **182 / 0 / 1 ignored**.
Clippy initially found the old PC constant unused; the shared validator now
reuses it, with final Clippy and the unchanged eight-case pair passing.

## Combined validation and retained failures

The first combined library run was **1,309 PASS / 2 FAIL / 1 ignored**.
Both failing platform cases also created PC=0 SQs; correcting three setup
operands, including the post-reset queue, preserves their media/reset checks.
The repaired combined Debug library passes **1,311 / 0 / 1 ignored**.

Independent source reviews and focused formatting, lint and budget checks
pass. Integration resolved two budget-file append conflicts as independent
row unions; existing ceilings were never raised. All reviewed runtime blobs
matched their lane sources before the additional platform fixture correction.
Combined full-project and exact-SHA hosted validation are still required.
No criterion, known defect, product state or guest-contract deviation is changed.

The preceding PR 322 is separately complete on main `ebd035b6`: hosted full
run `37411865569` passed 44 stages and manual CI `37411867835` passed 14 jobs;
main CI `37414807126` passed. Its local **37 PASS / 8 FAIL** result remains
failed, with sandbox/host integration limitations recorded privately. Those
prior hosted results do not validate this successor source.
