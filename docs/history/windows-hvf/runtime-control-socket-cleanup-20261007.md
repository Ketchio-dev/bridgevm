# Runtime-control socket ownership cleanup — 2026-10-07

The native display runtime-control listener captured `fstat(fd)` identity
following `bind`, then compared it with `lstat(path)` before unlinking during
cancellation. On macOS a Unix socket descriptor and its filesystem pathname
have different device/inode identities. A disposable host probe reproduced
the inequality; the existing guard therefore left the server's own socket.
This does not attribute every historical leftover to this single defect.

The server now captures the bound pathname identity and verifies socket type,
while preserving identity-matched conditional removal. The full Darwin
runtime-control block is extracted from the launcher rather than growing its
structural ceiling. The accept handler captures its source's descriptor rather
than reading a mutable property concurrently with stop.

An optional asynchronous stop completion observes listener close and pathname
cleanup completion on the existing serial queue. It does not drain accepted
clients, guarantee unlink success, or make application/process exit wait.
Lifecycle operations remain caller-serialized; no concurrent restart contract
is introduced. A dispatch group remains observable across repeated stop calls.

Tests wait for cancellation before asserting that an owned path is absent or
that a replacement regular file/socket survives. The replacement socket still
accepts a connection. Fixture paths register exact teardown cleanup at creation
without a global `/tmp` scan or product socket-path change. Normal/failed start,
never-started and repeated-stop completion are covered.

Focused Apple XCTest passes **17 tests**. Restoring the original `fstat`
capture makes the new owned-socket assertion fail (`lstat` returns zero rather
than ENOENT); the correct implementation is restored. An initial test build
failed due to unqualified `bind` resolving to an XCTest method; explicit
`Darwin.bind` repaired it. Full-project and exact-SHA hosted checks are pending.
No Windows, display smoothness, live-gate or release criterion claim is made.
