# Structural refactor — handoff

Read this before touching module structure. It records what is already done, what
is left, and the traps that silently deleted code during the work so far.

State at handoff: `main` = `c521f10`, working tree clean.

## Boundaries

- Source root is `/Users/insighton/Projects/bridgevm`. Work only here.
- `/Users/insighton/BridgeVM` is a **separate live-test and evidence workspace**.
  It is not the main repository. Do not delete, rename, or reorganize its VM
  disks, run directories, private state, or historical evidence.
- `CLAUDE.md` is a local secret-bearing file. Never `git add`, commit, push,
  quote, log, or screenshot it, and never reproduce the API key it contains —
  not in command-line arguments, not in PR text. Commit with
  `git add -A ':!CLAUDE.md'` and verify with
  `git diff --cached --name-only | grep -q '^CLAUDE.md$'` before every commit.
- Do not change public APIs, runtime behaviour, evidence formats, or
  fail-closed security behaviour. Do not mix a behaviour change and a structural
  change in one commit.

## The standard

1. No file exceeds 1,000 lines.
2. Module names describe a **responsibility**. `part_N`, `impl_N`, and "named
   after whichever item happened to land first in the file" are all forbidden.

## Already done — do not redo

Merged as PRs #32–#39. No `_impl_N`, `part_N`, or first-item-named module
remains anywhere in the workspace.

| area | result |
|---|---|
| `nvme` | 15 responsibility modules |
| `virtio_gpu` | 21 — `magic_value_impl_2..6` became protocol / virtqueue / scanout / fence / vblank / trace / … |
| `platform_virt` | 19 — machine_assembly, mmio_dispatch, interrupt_routing, one module per device family |
| `pcie`, `virtio_blk`, `virtio_console`, `virtio_net` | 44 — the four share one vocabulary |
| `virtio_gpu_3d` | 15 |
| `bridgevm-daemon`, `bridgevm-tools-linux` | 35 |
| `config`, `agentd`, `qemu`, `apple-vz` | 40 |
| `bridgevm-storage` | 32 — the 2,036-line single `impl VmStore` (78 methods) split into 17 responsibility-scoped `impl VmStore` blocks |

**No function body was changed in any of it.** Every one of those PRs is a pure
move: which file an item lives in, and what that file is called.

## Remaining work — four files, two different kinds

### A. Solvable by moving items (same approach as the merged PRs)

```
5365  crates/bridgevm-hvf/examples/hvf_gic_boot_probe.rs
3232  crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console.rs
```

An example root is a **crate root**: `mod x;` inside `examples/foo.rs` resolves
to `examples/x.rs`, not `examples/foo/x.rs`. That is why the existing
hand-written modules carry `#[path = "hvf_gic_boot_probe/agent_console.rs"]`,
and every generated part needs an explicit `#[path]` too.

Also: every `.rs` directly under `examples/` is its own example target, so the
parts must **not** move up beside the root. They belong in
`examples/hvf_gic_boot_probe/`.

### B. Not solvable by moving items — needs real function decomposition

```
2844  crates/bridgevm-hvf/src/platform/apple/firmware_run_loop.rs   (one ~2,450-line function)
1091  crates/bridgevm-hvf/src/windows_arm/run_loop_render.rs        (render_text, ~1,000 lines)
```

The file is not large; a single *function* is. Fixing this means extracting
helpers, which breaks the "no function body changed" guarantee held everywhere
above. Therefore:

- Do it in **separate PRs**, never mixed with (A).
- Each extraction must be behaviour-preserving, and tests must run after each step.
- `firmware_run_loop` is the HVF run loop and is order- and timing-sensitive.
  Split it only by passing existing state through to helpers. Do not reorder or
  restructure control flow.

## Verification — all of this must pass before every commit

```bash
cargo fmt --all
cargo check --workspace --all-targets     # zero errors, zero warnings
scripts/check-refactor-budgets.sh          # PASS
git diff --check                           # no trailing whitespace / CR
```

Test counts must be **exactly** these. Any difference means something was lost:

| package | tests |
|---|---|
| `bridgevm-hvf` | 738 (740 with `--features venus`) |
| `bridgevm-config` | 16 |
| `bridgevm-agentd` | 33 |
| `bridgevm-qemu` | 50 |
| `bridgevm-storage` | 67 |
| `bridgevm-apple-vz` | 44 |
| `bridgevm-agent-protocol` | 13 |
| `bridgevm-daemon` | 47 |
| `bridgevm-tools-linux` | 81 |
| `displayd` | 20 |
| `hvf-runner` | 14 |
| `lightvm-runner` | 16 |

`scripts/refactor-budgets.tsv` is a ratchet. Regenerate it before committing.
Lower a ceiling only when a file actually shrank; never raise one.

## Traps that silently deleted code during this work

Every one of these caused real loss while moving Rust source automatically. They
are listed because none of them was caught by reading the output — each was
caught by compiling or by a round-trip check.

1. **Raw strings.** Counting `{}` inside `r#"..."#` as code throws off every item
   boundary after it (`bridgevm-config` embeds a whole JSON Schema in one).
   The mirror-image mistake is just as bad: the `r"` at the end of
   `"...--apple-vz-runner"` is not a raw-string opener. An `r` opens a raw string
   only at a token boundary.
2. **`macro_rules!`.** `macro_rules!\b` never matches — `!` is not a word
   character — so the macro is not seen as an item and vanishes. Separately, the
   module defining a macro must be declared `#[macro_use]` **before** every
   module that expands it; `macro_rules!` resolves in textual order.
3. **Multi-line attributes.** `#[command(\n … \n)]` ends with `)]`. A terminator
   that only accepts `;` or `}` runs on and swallows the `struct` that follows.
4. **Trailing line comments.** `const MASK_REG: u64 = …; // low 12 bits` hides
   the terminating `;`, so the item runs on and absorbs the next several items.
5. **Attributes on the first item.** A header scan that stops at the first
   non-`use` line will have already eaten the leading `#[derive(...)]` and `///`
   lines belonging to that item.
6. **`cargo fix` judges one target at a time.** It will delete an import the lib
   does not use but the test module in the same file does. Verify import cleanup
   against `--all-targets` and roll back on failure.
7. **cfg-gated code is invisible until you turn the cfg on.** `--features venus`
   and the non-macOS `platform` fallback both hid broken trees behind a clean
   `cargo check`. "0 errors" does not mean "that file compiled".

**Strongly recommended:** before moving anything, run a round-trip check — parse
each file, reassemble its items in the original order, and require an exact match
with the original. All seven of the above were found that way.

The tooling that did this work is in `scripts/refactor/` — see its README. It
already handles all seven traps, and `roundtrip.py` is the check itself.

Two more habits that paid off:

- Refuse to write anything unless **every** parsed item and method has been
  assigned a destination. An unassigned item must be an error, never a silent drop.
- When an import is genuinely used only by tests, do not silence the lint and do
  not widen the item to `pub`. Let the test file import the module directly, so it
  states its own dependency.

## Git discipline

One focused branch per piece of work. Verify, push, open a PR, confirm it is
mergeable and clean, then merge. Preserve unrelated and untracked user files.

Commit messages should say what changed and why — in particular, what defect was
found and what it had broken. That is the part worth reading later.
