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
| Documentation system | PASS; 190 classified documents and 412 links |
| First integrated project check | FAIL only expected capability freshness; every other gate completed |
| XCTest shim suites in that run | 425, 801 with two required live-only skips, and 62 PASS |

The first journey expectation assumed the installed registration would reach the
test's deliberately rejected publication callback. It instead reached the later
exact-configuration gate and returned `configurationMismatch`, because the shared
safety fixture rewrites the synthetic `swtpm` path. The failed assertion is
retained. The corrected contract requires that exact later refusal and one runtime
session creation, proving it crossed the earlier install-pending boundary without
starting a process or reading a key.

The first full project check correctly rejected stale capability identity because
the new native source followed the previous tested commit. All executable,
documentation, budget and shim steps still completed successfully. The registry
is resealed to the exact source commit before the corrected final project run.

## Evidence limit

The journey uses synthetic files, injected host models and retained operation
state. It proves deterministic identity binding, terminal retry admission,
install-pending refusal, installed-state routing and existing stop target-binding
contracts. It does not install Windows, launch App.main, render a WindowServer UI,
access Keychain, boot a guest, observe guest shutdown or prove physical-hardware
behavior. No live queue job was submitted. Exact-head hosted CI and post-main
verification remain required before integration. No capability state, threshold,
known defect, product wording or machine-contract deviation is promoted.
