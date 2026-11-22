# Integration Tests

Executable smoke tests and contracts for the own-HVF CLI, runner, packaging and
app paths live here. Most are metadata-focused so they can run without booting a VM.

Run the current smoke coverage from the repository root:

```sh
tests/integration/hvf-windows-installed-boot-p3-gpu-policy-smoke.sh
tests/integration/hvf-windows-installed-boot-ramfb-display-cadence-smoke.sh
tests/integration/hvf-runner-installed-boot-launch-policy-smoke.sh
```

`hvf-windows-viogpu3d-render-stage-smoke.sh` is intentionally parser-only: it
uses synthetic ARM64 PE headers and a dummy catalog to prove the pinned staging,
INF registration, CopyFiles, stale-metadata rejection, and finalizer ordering
contracts. It does not claim InfVerif, Inf2Cat, SignTool, or live Windows proof.
`hvf-windows-viogpu3d-build-kit-smoke.sh` statically verifies the pinned x64
Windows → ARM64 clang-cl build kit; the actual PowerShell/WDK run remains an
external gate.
