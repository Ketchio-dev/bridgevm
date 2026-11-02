# Input host fixture compatibility with Rust1.85 (2026-11-02)

Required CI41394337862/job125112332995 failed on Rust1.85.0 because the new
held-writer fixture called unstable std::fs::File lock/unlock methods.
Repair source bc465b83 reuses InputControlFile.with_exclusive to hold the
existing pinned-file RAII guard, releasing it before joining the reader even
after a timeout. The five-second causal deadline and all recovery/replay
assertions remain; production code, unsafe sites and APIs are unchanged.
Both host-preparation regressions pass on installed Rust1.85.0.

The added public history note then exceeded that document's unchanged
28-line ceiling (37 lines), failing budget CI41397883498/job125123077833.
This follow-up extracts that note into a classified history document;
existing ceilings are not raised. The original input history links here.

Superseded local sessions71311 and41967 were each stopped with observed
exit143 before changing their heads and are retained as aborted, not PASS.
Original failed CI logs remain recorded. Replacement exact local/hosted/PR
gates are still required; no criterion, product-state or live guest claim
is promoted. A11 remains OPEN.
