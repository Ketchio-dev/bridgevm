# Confirmed force stop during owned guest grace (2026-11-02)

Normal owned Stop already has an independent cancellation channel, but a
user could not explicitly shorten its180second guest-shutdown grace.
Runtime, dashboard and retained Stop surfaces now share a Force Stop control
only while the session is stopping and the operation is guestGrace.

Opening confirmation saves the run UUID/PID and stop operationID. Confirming
uses that saved target; session and controller revalidate identity, operation,
phase and deadline. A stale dialog never substitutes a newer run. The warning
states that unsaved guest work may be lost.

The accepted request reuses cancelRunner and the same operation. It sends
one owned stop frame, shortens observation to at most the existing12seconds,
and retains the exit/EOF/cleanup ownership fence. Repeated or nonwaiting
requests are refused without renewing the deadline. Forced cancellation
remains guest-shutdown-unproven; it is not normal-shutdown evidence.

Availability is published through the existing runtime observer only when
its value changes. No new poller is added. The ordinary Stop disabled state
does not disable the separate escape. Request feedback clears after leaving
stopping or changing run identity; captured confirmation targets stay pinned.

Four focused shim XCTest cases pass after the app module/new UI compile:
exact escalation/fence/deadline, stale/closed phases, availability publication
and captured confirmation against a replacement run. Existing keychain API
deprecation warnings remain. No real VM, WindowServer or operator process was
used. Final exact project/hosted/PR gates remain required; criteria/state stay.
