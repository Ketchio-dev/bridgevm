# Contributing to BridgeVM

Thanks for helping make native virtualization on Apple silicon easier to use
and easier to verify. Bug reports, documentation, tests, focused fixes, and
carefully scoped features are all useful.

## Find a place to start

- Browse [good first issues](https://github.com/Ketchio-dev/bridgevm/labels/good%20first%20issue).
- Fix an unclear instruction or broken diagnostic before taking on a device
  model.
- Add a deterministic regression test for a bug you can reproduce.
- Improve an error message without widening the accepted security state.
- Read the [current status](STATUS.md) before proposing a capability claim.

Use the issue forms for bugs and feature proposals. Report security
vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Set up a development checkout

BridgeVM development targets an Apple-silicon Mac with macOS 14 or newer. The
workspace requires Xcode/Swift and Rust; the minimum Rust version is 1.85, while
the deterministic project gate currently defaults to the pinned 1.97 toolchain.

```sh
git clone https://github.com/Ketchio-dev/bridgevm.git
cd bridgevm
cargo +1.97.0 build --workspace --locked
swift build --package-path apps/macos
scripts/check-project.sh --fast
```

QEMU is optional unless you work on the Compatibility Engine. Native Venus,
packaging, and live Windows work have additional dependencies documented beside
their scripts; do not install them for an unrelated documentation or Rust-only
change.

## Choose the right area

| Area | Main paths | Useful first check |
| --- | --- | --- |
| Rust product/API | `crates/bridgevm-{api,cli,core,config}/` | `cargo test --workspace --locked` |
| Windows HVF VMM | `crates/bridgevm-hvf/` | `cargo test -p bridgevm-hvf --locked` |
| Runtime lifecycle | `crates/bridgevm-hvf-runtime/`, `runners/` | focused crate tests |
| macOS UI | `apps/macos/` | `scripts/run-swift-tests.sh` |
| Packaging | `packaging/macos/`, release scripts | relevant integration smoke plus full check |
| Documentation | `README.md`, `STATUS.md`, `docs/` | `bash scripts/check-documentation-system.sh` |
| Live evidence | `scripts/live-gates/` | policy smoke first; real runs use the Studio queue |

The [documentation index](docs/README.md) separates current product guides,
plans, decisions, and historical evidence.

## Make a change

1. Create a focused branch from current `main`.
2. Reproduce the problem or state the expected behavior before editing.
3. Read [AGENTS.md](AGENTS.md). It is the repository's binding evidence and
   safety policy.
4. For a change spanning three or more files, make sure an approved `PLAN.md`
   exists before implementation. `PLAN.md`, `GOAL.md`, and `HANDOFF.md` are
   operator-owned and must not be staged.
5. Add or update a test at the lowest layer that can catch the regression.
6. Run focused checks while iterating, then `scripts/check-project.sh` before
   calling the change complete.
7. Open a pull request that says what is now known, not only what was edited.

Keep each pull request reviewable. A refactor, behavior change, and capability
promotion usually deserve separate conclusions.

## Evidence levels

BridgeVM does not equate a green unit test with a working Windows guest:

1. live gate receipt at the stated sample count on real hardware;
2. live single run;
3. deterministic automated test;
4. static reasoning.

Use the strongest evidence you actually have and no stronger wording. Never
lower a threshold, rewrite a failed experiment into a pass, or use one live run
to close a multi-run gate. Capability wording comes from
[`capabilities/windows-hvf.json`](capabilities/windows-hvf.json).

Most contributions need only deterministic tests. Real Windows media, a GPU,
WindowServer, or bare-metal Hypervisor.framework work belongs in the local
Studio queue; it never runs on a public pull-request runner. See the
[development system](docs/development-system.md) for the full workflow.

## Checks

Fast deterministic feedback:

```sh
scripts/check-project.sh --fast
```

Focused Rust checks:

```sh
cargo +1.97.0 fmt --all --check
cargo +1.97.0 clippy --workspace --all-targets --locked -- -D warnings
cargo +1.97.0 test --workspace --locked
```

Focused Swift checks:

```sh
swift build --package-path apps/macos
scripts/run-swift-tests.sh
scripts/run-xctest-shim-suites.sh
```

Required final deterministic gate:

```sh
scripts/check-project.sh
```

Do not move an ordinary deterministic check to a private machine to hide a CI
failure. Hosted CI and Security and quality must pass for the exact pushed SHA.

## Safety rules that commonly surprise contributors

- Never commit Windows media, VM disks, UEFI vars, vTPM state, recovery keys,
  proprietary title content, or unredacted live receipts.
- Canonical VM images are immutable. Live work uses `cp -c` clones, with a
  separate writable disk and vars file for every parallel lane.
- Files under `scripts/win-assets/**` keep CRLF line endings.
- Structural limits in `scripts/refactor-budgets.tsv` are a ratchet. Do not
  raise an existing ceiling; extract a module and register new files at their
  actual size.
- Security paths fail closed. A missing signer, helper, hash, or provenance
  record is an error, not permission to continue.
- Test-signed Windows graphics packages belong only to Graphics Lab. They are
  never production signing evidence and never enter the General Preview.

## Pull request checklist

The pull-request template asks for:

- the user-visible or engineering outcome;
- the tests and evidence level;
- security, data, compatibility, and packaging risks;
- capability/documentation impact;
- confirmation that private artifacts and operator-owned files are absent.

Maintainers may ask for a smaller change, a failing test, or a real-hardware
receipt when the claim requires it. That is evidence calibration, not a request
to make the threshold easier.

By contributing, you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
Contributions are accepted under the repository's [Apache-2.0 license](LICENSE).
