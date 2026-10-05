# HDA CORB memory-error interrupt enable — 2026-10-05

Integrated source checkpoint `5f0cde554521b2a7afcb9bcf97c3a40ed247c5c6`
repairs the missing CORB memory-error interrupt enable check. An unbacked
command-ring fetch previously raised the interrupt contribution even when
`CORBCTL.CMEIE` was clear.

## Interrupt source and architectural scope

[Intel High Definition Audio Specification 1.0a](https://www.intel.com/content/dam/www/public/us/en/documents/product-specifications/high-definition-audio-specification.pdf)
sections 3.3.22–3.3.23 define CMEIE and sticky write-one-to-clear CMEI.
Section 3.5 describes the source-enable, CIE and GIE interrupt tree;
section 4.4.1.3 permits CORB initialization with CMEIE disabled.

The repair gates only CMEI's contribution to the interrupt source with CMEIE.
Sticky status, CORB operation, CIE/GIE and existing PCI MSI mechanics remain
unchanged. `interrupt_sources.rs` extracts the existing helper; reversing the
new predicate and extraction recovers the original controller byte for byte.
The controller ceiling falls from 752 to 733; the new helper and fixture have
actual ceilings 23 and 223. All old HDA tests and prior command/ring-address,
PCM-overflow and RUN-resume repairs remain intact; no ceiling increases.

## Paired public-MMIO experiment

Six unchanged new fixtures configure PCI BAR and standard MSI through the
public platform route, then issue MMIO that reaches an unbacked CORB fetch.
The failed fetch latches CMEI without advancing CORBRP or RIRBWP. Internal
controller access only observes the interrupt level. The existing 30 methods
and four new controls pass on the baseline; masked fault and latched-error
mask/unmask fail: 34 PASS / 2 FAIL. Fixed debug and release each give
36 PASS / 0 FAIL, with the same 36 named methods and identical fixture bytes.
Fixture SHA-256:
`072e89fceec193f4d50117df17e5422ecc5f098aec030e3c6f62d51e3da8d188`.
Retained raw SHA-256 values are:

- Baseline: `32e085aa40edf955e247b2c413eb4383946eedd849095e9a2cf3332bb8ccf411`.
- Debug: `69ebc966435b4517ba7c02e6150d47562294c8e7e9afa87cf40f2f1c1cdb8a64`.
- Release: `4b89d83e043c92af1cb1ec07723325eff2ac6f05cf9f5609923522f3eebda6d4`.

The fixtures verify masked status without level/MSI, exactly one programmed
MSI when enabled, delayed unmask/remask/unmask, CIE/GIE masks, stopped-CORB
W1C clearing and a zero-write control. Valid command/RIRB recovery follows
CRST and full ring reinitialization. Clearing status alone is not treated as
permission to continue command operation after the fault.

Focused Clippy, formatting, budgets and whitespace checks pass. Independent
source/paired-result review clears the narrow integration. Baseline failures
remain recorded; there were no fixture corrections or unexpected test/check
failures. The peer review separately retains an initial Git tree command that
could touch cache metadata despite read-only scope; later read-only validation
confirmed the frozen source/index/tree. Integration's budget-append conflict
was resolved as an exact row union with no raised ceiling. All four nonbudget
files match the reviewed source.

## Limits and incomplete integration evidence

Existing INTSTS treatment and pending-MSI semantics are outside this repair;
this is not broad HDA conformance evidence. There is no live guest, Windows,
physical MSI, audio continuity or audio-quality result.

The parent local project check remains FAILED at 37 PASS / 8 FAIL. The parent
hosted campaign remains FAILED/INCOMPLETE, with verified runner-acquisition
failure and no actual full-project result at this checkpoint. Successor full
project and exact-SHA hosted checks remain required. Criterion states,
thresholds, known defects and product wording are unchanged; no live criterion
or release promotion is claimed.
