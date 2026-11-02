# Concurrent control command appends (2026-11-02)

The GUI and backend sought to the control file's end before writing. Owned
shutdown appends independently on its background queue. A shutdown arriving
between that seek and write could be overwritten while both writers reported
success. Stop's input cleanup can overlap the background request.

GUI/backend commands now share an O_APPEND writer, so the kernel chooses EOF
for the write itself. It performs one checked write and reports a partial or
failed write without retry. This is concurrency-safe append positioning,
not an all-or-nothing transaction for a partial write. Command normalization,
service admission and creation policy remain with the callers.

The owned shutdown writer retains its captured identity, one-attempt latch,
close latch and no-retry behavior. No new channel identity policy is added.

A narrow write dependency let the real owned shutdown append after the GUI
writer prepared its descriptor. The extracted seek/write baseline lost the
shutdown bytes: four tests passed and the causal test failed. With O_APPEND,
the same five direct XCTest shim cases passed with exit zero and no skips.
Partial-write and owned-shutdown refusal/close behavior are included.

The tests use an owned temporary file. Session/backend app compilation and
the new service/create/failure integration case remain in the required final
project gate. No guest shutdown, Windows criterion or product-state proof is
claimed from this test.
