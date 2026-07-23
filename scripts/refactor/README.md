# Responsibility-regroup tooling

These moved ~1,900 items and methods across the workspace (PRs #32–#39) without
changing a single function body. See `docs/refactor-handoff.md` for what is done,
what is left, and why each safeguard here exists.

Nothing here rewrites code. Every tool only decides *which file* an item lives in.

## The one rule that makes this safe

`regroup.py` refuses to write anything unless **every** parsed item and method
appears in the spec. An unassigned item is an error, never a silent drop — the
mechanical splitter this replaced deleted a `macro_rules!` and a
`#[derive(Parser)]` exactly by dropping what it had not classified.

## Order of use

```bash
# 0. ALWAYS FIRST. Parse each file, reassemble its items in the original order,
#    require an exact match. If this is not clean, nothing below is trustworthy.
python3 scripts/refactor/roundtrip.py crates/<crate>/src/<module>

# 1. Dry run: lists what is unassigned, or the resulting line count per target.
python3 scripts/refactor/regroup.py crates/<crate>/src/<module> spec.json --all

# 2. Apply once the plan covers everything and no target is over budget.
python3 scripts/refactor/regroup.py crates/<crate>/src/<module> spec.json --apply

# 3. Each target receives the union of the source headers; drop what it does not
#    use. Verified against --all-targets and rolled back if it breaks anything.
python3 scripts/refactor/drop_unused_glob.py <package> [--features x]

# 4. Settle mod.rs / lib.rs / main.rs re-exports (see below).
python3 scripts/refactor/fix_mod_reexports.py <package> crates/<crate>/src/<module> [root.rs]
```

Then the usual gates: `cargo fmt --all`, `cargo check --workspace --all-targets`
(zero warnings), `scripts/check-refactor-budgets.sh`, `git diff --check`, and the
per-package test counts in `docs/refactor-handoff.md`.

## Spec format

```json
{
  "targets": { "protocol.rs": "One-line description of the responsibility." },
  "items":   { "REG_MAGIC": "protocol.rs", "VirtioGpu": "device.rs" },
  "methods": { "VirtioGpu::read_common": "config_space.rs" },
  "root":    "lib.rs",
  "keep":    ["trace.rs"]
}
```

- **`items`** — top-level items, keyed by name. A marker impl is keyed
  `"impl Send for BlobScanoutMapping"`. A `use` sitting among the items is keyed
  by its own text.
- **`methods`** — keyed `Type::method`. For a trait impl the key is
  `"Default for VblankWakeState::default"`; the type text is whatever follows
  `impl` on the line, so generics are included: `VirtioNet<B>::read_common`.
- **`root`** — the crate root for a lib/bin split (`lib.rs` / `main.rs`). Omit
  for a `mod.rs` module directory. A crate root cannot be a directory, so its
  modules are siblings and the declarations live in the root file itself.
- **`keep`** — modules already named for their responsibility. They are left
  untouched and their declarations survive in the root.

Write the spec as a small Python file that emits the JSON; the item lists are
long and a generator keeps them readable. Get the exact key names by running
`regroup.py` with an empty spec and `--all` — the unassigned list *is* the
inventory.

## Re-export shapes, and why they differ

`fix_mod_reexports.py` derives the right line per module:

| module contains | root line |
|---|---|
| `pub` items | `pub use m::*;` |
| only `pub(crate)` items | `pub(crate) use m::*;` |
| only impl blocks | nothing — an impl exports no names |

A `pub(crate) use m::*;` whose items only the **test** module consumes is
reported unused by the lib target, and deleting it breaks the tests. The tool
does not silence the lint and does not widen the items to `pub`. It drops the
glob, makes the module `pub(crate) mod`, and adds an explicit import to the test
file — which then states its own dependency.

## What the parser handles, and why

Each of these silently lost code before it was handled. Do not remove them.

- **Raw strings** are masked before any boundary is computed (`bridgevm-config`
  embeds an entire JSON Schema in one `r#"..."#`). An `r` opens a raw string only
  at a token boundary — the `r"` ending `"...--apple-vz-runner"` does not.
- **`macro_rules!`** is matched without `\b` (`!` is not a word character), and a
  module defining one is emitted `#[macro_use]` ahead of the modules that expand
  it, because `macro_rules!` resolves in textual order.
- **Multi-line attributes** end when their own brackets balance. `#[command( … )]`
  closes with `)]`, which an item terminator rejects.
- **Trailing `// comments`** are stripped before checking whether a line ends the
  item; otherwise a `const X = …; // note` swallows what follows.
- **Attributes and doc comments on the first item** belong to that item, not to
  the file header.
- Braces are counted ignoring comments and string/char literals, and parens and
  brackets are tracked too — a method signature spans lines before its `{`.
- `extern "C" fn`, associated consts and types, and marker impls with no methods
  are all named items.

`drop_unused_glob.py` never touches `mod.rs` / `lib.rs` / `main.rs`: those
re-exports are structural, and removing one is not an import cleanup. It also
avoids `cargo fix`, which judges one target at a time and will delete an import
the lib does not use but the test module in the same file does.
