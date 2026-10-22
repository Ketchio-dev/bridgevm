# Twelfth candidate integration — 2026-10-21

This records automated tests and source integration. Product state, capability wording,
criterion states, thresholds and known defects remain unchanged. No guest, physical UI,
live queue or release action is part of it.

## Contents

The twelfth candidate stacks on the eleventh (#421). The eleventh's seal `6e8e558c` passed
all 45 local stages, hosted full run 39570473094 and all 101 associated workflows. Two
earlier drafts are merged unchanged:

- **#417 counted AX relationships**, head `e347238a` (source `c2ebbb6b`).
- **#418 nontext clipboard ownership**, head `9c3d2d26` (source `cd0eb9cc`).

Their merges conflicted only in the registry, its rendered blocks and budget rows. Neither
draft added a history record; their A11 notes are preserved verbatim in the
[replaced A11 notes archive](../../archive/a11-measured-20261015-20261021.md).

Neither draft previously had a complete local run with Apple XCTest available. #417's VM
run failed 2 of 45 stages because XCTest was missing there, and that failure stays recorded
in its pull request.

Two follow-ups complete the candidate:

- **Retained slot guidance and release** (`710c1084`). While a terminal retained control's
  record stays open, delete, clone and move refuse because that control still holds the
  slot. The refusal advised stopping the VM and refreshing the list, but generic VM screens
  have no list refresh and the VM had already stopped. Start refused with a note that named
  no way out. The refusals now name the release action: close the item with '목록에서 닫기'
  under '진행 중이거나 종료된 작업' after its run ends. Closing a generic record now
  resolves its slot through the ordinary lookup at once; before, that happened only when
  some view next looked the control up. The admission checks are unchanged, and stale
  backends stay unreachable.
- **Storage test cleanup** (`3f8a7245`). A passing import test left the imported copy's
  metadata directory at mode 0500, so a plain recursive delete of its temporary directory
  failed with Permission denied. The test now restores that mode; its assertions are
  unchanged.

## Focused results

| Check | Before | After |
| --- | --- | --- |
| `RetainedTerminalGuidanceTests`, native | 2 methods, 10 failed assertions | passes |
| 12 retained-control suites, native | not run | 45 tests, 0 failures |
| Native UI driver contracts, merged #417 | not run | PASS, including 111 wrapper and 91 empty-relationship checks |
| Probe example tests, merged #418 | not run | 627 passed, 0 failed |
| Restricted import test, then `rm -rf` | passes, but 1 read-only directory left and `rm` refused | passes, 0 left, `rm` succeeds |
| Storage library | not run | 123 passed, 0 failed, no read-only directory left |

## Limits

- The guidance is SwiftUI value and model tests only. No rendered window, click or
  Accessibility path is exercised.
- #417 still needs its physical UI pilot. #418 used modeled pasteboards and guests only.
- Other refusals still advise refreshing the list where generic screens offer no list
  reload. This candidate does not change them.

The new head requires its own complete local and hosted checks.
