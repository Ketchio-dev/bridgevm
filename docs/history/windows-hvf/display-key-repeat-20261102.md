# Held-key repeats no longer pile up on the ordered route (2026-11-02)

macOS repeats a held key on its own clock. On the negotiated ordered input
route each repeat became one KEYINPUT ticket; the queue holds up to 64 for
30 seconds, coalesces only pointer moves and advances only when the guest
acknowledges, which the app reads at its 100 ms poll. A held Backspace or
arrow could therefore keep acting in the guest after the key was released.

A physical-key auto-repeat is now admitted only while at most the ticket in
flight waits and no clipboard paste is pending; otherwise it is absorbed and
nothing is written. A key's first press is never absorbed, and the legacy
route writes every repeat as before. Repeats of held letters, which arrive as
committed text through the input method, are unchanged.

Evidence is automated tests. With the new case on the previous code, 20
repeats behind one in-flight ticket left 21 queued (expected 2). With the
change, the key-repeat, display-input-failure and text-input suites pass,
21 of 21. The guest round-trip time and the felt effect were not measured
live; no criterion or product state changes.
