# Native app CLI discovery diagnostics — 2026-09-20

Source commit `7688f3f9bc45ebadd0a4878e6a68ea2f57ff33cf` adds the read-only
`bridgevm app doctor` command. It inspects the same bounded executable candidates
and `bridgevm-native-cli-v1` compatibility marker used by native app command
forwarding, then reports the first candidate that controls selection.

The diagnostic preserves fail-closed ordering: an installed incompatible or
invalid earlier candidate blocks a compatible later candidate. Text and
`bridgevm.app-doctor.v1` JSON output disclose that code signing, VM library
contents and guest health were not checked. The command never opens the app,
creates a library or contacts a VM.

Focused validation passed 156 CLI unit tests and 21 CLI integration tests. The
complete `scripts/check-project.sh` run passed for the source commit, including
structural budgets, workspace tests, native CLI/runtime contracts, Swift build,
853 BridgeVMControl shim tests with two declared skips, 62 AppleVzRunnerCore
tests and 115 BridgeVMProductE2E tests. The installed signed app was observed as
the selected compatible candidate during a local JSON smoke run.

This is deterministic host-tooling evidence only. It does not prove a Windows
boot, a clean-machine app journey, code-signing validity, or any live release
criterion. A9, A11 and A19 remain OPEN, product state remains
`ENGINEERING_PREVIEW`, and 3D remains an experimental future path.
