# Framebuffer image retry — 2026-10-04

Classification: deterministic tests and static source review. The
[capability registry](../../../capabilities/windows-hvf.json) remains authoritative:
ENGINEERING_PREVIEW and A9, A11 and A19 OPEN are unchanged. No guest behaviour or
release criterion is proven by this record.

## Failure and production repair

The framebuffer view previously consumed a published sequence before constructing
its data provider and image. A construction failure therefore prevented the next
display tick from retrying the same frame, even though no image had been presented.

[C1 `c1bf393f9f3d660c5c170f4f9d67483e2278dd4d`](https://github.com/Ketchio-dev/bridgevm/commit/c1bf393f9f3d660c5c170f4f9d67483e2278dd4d)
commits the sequence only after assigning a successful image to the layer.
The even-sequence checks before and after the pixel copy remain unchanged.
Failed provider construction frees the copied buffer directly; after provider
creation, the provider's release callback owns that buffer, including image failure.

Production sources are [the view](../../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferView.swift)
and [the image factory](../../../apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferImageFactory.swift).
[Retry tests](../../../apps/macos/Tests/BridgeVMControlTests/HvfFramebufferRetryTests.swift)
exercise the real view with a small synthetic mapped framebuffer: one provider or
image failure, a successful second tick at the unchanged sequence, exact rendered
pixels and accessibility value, then a third tick that performs no repeated decode.
These tests use no Windows guest or live GPU workload.

## Retained checks and hosted failures

| Check | Actual result | Evidence scope |
| --- | --- | --- |
| Before-fix retry baseline | 2 tests, 2 failures | Both second-tick image assertions failed |
| Corrected native focused suites | 10 tests, 0 failures | Includes both retry cases and existing framebuffer/accessibility cases |
| C1 structural budgets | PASS | View ceiling 447 to 424; new factory/test ceilings 28 and 78 actual lines |
| C2 display-click static contract | 4 tests, 0 failures | Source assertions only; no native click or guest input proof |
| Current full project check | PENDING | Focused checks do not replace this required check |
| Required hosted CI for the successor head | PENDING | C1 failures do not become successor passes |

C1 hosted [capability/documentation validation](https://github.com/Ketchio-dev/bridgevm/actions/runs/37176507952/job/111360107086)
failed the existing freshness requirement after the framebuffer code changed.
The [macOS 15 app job](https://github.com/Ketchio-dev/bridgevm/actions/runs/37176507952/job/111360107079)
and [macOS 26 app job](https://github.com/Ketchio-dev/bridgevm/actions/runs/37176507952/job/111360107083)
also failed: the display-click static parser assumed the old exact initializer and
raised `IndexError` when the optional image-factory argument was present.
The C1 [manual full run](https://github.com/Ketchio-dev/bridgevm/actions/runs/37176511138/job/111360120299)
exited 1 with two failing outer steps: freshness and the same static parser error.

## Parser successor and remaining evidence

[C2 `a5744ac26365158f0f6c02e5467755e9bbd9a916`](https://github.com/Ketchio-dev/bridgevm/commit/a5744ac26365158f0f6c02e5467755e9bbd9a916)
changes one extraction expression in the
[display-click contract](../../../tests/integration/t17-display-click-contract.py).
It retains the exact session initializer prefix while permitting same-line optional
arguments; the existing accessibility, frame-ready and pointer-order assertions
remain unchanged. The old parser failure was also reproduced locally before this
repair. The corrected four static cases passed, and the file remains at its
existing 54-line ceiling.

Full current-source local validation, exact-head hosted validation and capability
freshness sealing remain pending. This repair does not close A11, establish guest
rendering quality or performance, or supersede any earlier failed experiment.
