# Native app lifecycle guidance — 2026-09-16

Classification: historical deterministic evidence. Product state remains in the
[registry](../../../capabilities/windows-hvf.json). A9 and A11 stay OPEN and
ENGINEERING_PREVIEW is unchanged.

## Implemented boundary

Source `021d9a3723baf57dca001a072318bd5340c1d752` makes the human
installation result distinguish completion of the control exchange from Windows
installation completion. The versioned JSON schema, fields, phase values, exit
codes, operation identity, timeouts and app-owned installation behavior are
unchanged.

Human output now gives a next action from the retained phase. Active phases point
to an installation-status query, failed or cancelled terminal phases point to a
new explicit installation retry, and `done` points to readiness before start. The
text identifies the exact VM ID but does not construct a quoted shell command or
change argument boundaries. The canonical documentation connects create,
installation observation, terminal retry, readiness, start, status and stop.

## Retained deterministic validation

| Check | Result |
| --- | --- |
| Install guidance and unchanged JSON contract | 2 PASS |
| Pending/failure/replay/retry/installed-start journey | 1 PASS |
| Existing create, install, start and stop CLI selections | 14 PASS |
| Existing repository Swift harness | 16 PASS |
| Structural budgets | PASS without raising an existing ceiling |
| Documentation system | PASS; 191 classified documents and 413 links |
| Final integrated project check | PASS at metadata head `6e55630c` |
| XCTest shim suites in that run | 425, 801 with two required live-only skips, and 62 PASS |

The first journey expectation assumed the installed registration would reach the
test's deliberately rejected publication callback. It instead reached the later
exact-configuration gate and returned `configurationMismatch`, because the shared
safety fixture rewrites the synthetic `swtpm` path. The failed assertion is
retained. The corrected contract requires that exact later refusal and one runtime
session creation, proving it crossed the earlier install-pending boundary without
starting a process or reading a key.

The first full project check correctly rejected stale capability identity; all
other steps passed. The corrected run passed at `6e55630c`. PR167 exact head
`8ec0124e` passed 120 checks plus one intentional advisory skip and merged as
`72bcb5e1`; all 45 exact-main push workflows then passed.

## Evidence limit

The journey uses synthetic files, injected host models and retained operation
state. It proves deterministic identity binding, terminal retry admission,
install-pending refusal, installed-state routing and existing stop target-binding
contracts. It does not install Windows, launch App.main, render a WindowServer UI,
access Keychain, boot a guest, observe guest shutdown or prove physical-hardware
behavior. No live queue job was submitted. Hosted and post-main results prove
deterministic integration only. No capability state, threshold,
known defect, product wording or machine-contract deviation is promoted.
