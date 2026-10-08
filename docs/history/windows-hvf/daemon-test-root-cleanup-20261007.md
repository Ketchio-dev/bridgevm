# Daemon test-root ownership — 2026-10-07

The daemon's `temp_store()` returned a persistent `VmStore` pointing to a
PID/counter pathname. No scope owned that directory. Several successful tests
never removed their populated trees; explicit tail cleanup elsewhere was
bypassed by assertion panics. This explains a concrete source of `bvmd-*`
residue, not necessarily every historical directory.

A test-only directory owner now exclusively allocates mode700 directories,
refuses candidate collisions rather than adopting them, and retains the
root's device/inode identity. Scope exit removes only its matching directory.
A replacement or cleanup error fails normal execution; during unwinding it is
reported without causing a second panic. This is not a hostile-host rename
sandbox. The persistent production `VmStore` has no new deletion behavior.

Callers retain the owner independently of stores moved into `DaemonState` or
`daemon_request`. Returning fixtures carry it through to the test scope;
the fast lifecycle fixture owns it as its last field. Thus returned performance
artifact assertions still run before removal. Tests cover normal exit, unwind,
independent fixtures, candidate collision, replacement preservation and refused
socket-path admission.

## Short socket-root contract and preserved experiment

The first TMPDIR-based allocator passed82's predecessor81 tests with short
SSD scratch, but a longer external nested path failed nine Unix-socket binds
with SUN_LEN. That failed subprocess still left zero residual entries; it is
not a pass. Production helpers derive nested socket paths inside bundles.

The final test allocator preserves historical `/tmp` default behavior and
adds the explicit **test-only** `BRIDGEVM_DAEMON_TEST_ROOT` storage override.
An explicitly selected canonical root must fit the nested Unix socket byte
limit; an overlong selection fails clearly without an internal fallback or
skipped tests. SSD development checks set a short external root. Ordinary
cargo/hosted invocations retain their old short default rather than inheriting
macOS's much longer per-user TMPDIR. Production paths and protocols unchanged.

The final daemon suite passes **82/0**, clippy with warnings denied passes,
and an outside subprocess observer finds **zero entries** in the exclusively
owned external test parent after all tests exit. Full local and exact-SHA
hosted checks remain pending. No Windows or release criterion claim is made.
