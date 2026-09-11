# Windows-first Coherence integration gap

Date: 2026-09-11. This is a source investigation, not live capability evidence.
The current capability registry and open release criteria remain authoritative.

## Observed implementation boundaries

- The packaged Windows product is BridgeVMControl. A source search of that target
  found no Coherence, WINLIST or WINFOCUS integration.
- BridgeVMApp/Services/HvfGuestWindowProtocol.swift parses WIN records into
  GuestToolsWindowAction and builds WINBOUNDS, WINFOCUS and WINCLOSE commands.
  References to this type in the searched macOS app sources and tests are its
  definition and tests only. Its presence does not establish a runtime connection.
- The engine's agent_console/window_protocol.rs handles WINLIST records through
  WINEND and recognizes verb-specific mutation replies. It validates numeric
  record fields and base64 before logging records.
- WINLIST records contain handle, PID, desktop bounds and title. They do not
  contain a window image, ownership/dialog hierarchy, z-order, effective DPI,
  focus state, a VM generation, or a request nonce.
- The product's HvfFramebufferView currently presents the guest display through
  an IOSurface or display.fb. That is not evidence of independent guest-window
  capture, including windows obscured by other guest windows.

## Prior live primitive evidence

The [2026-08-17 primitive experiment](windows-arm/evidence/coherence-windows-live-20260817.md)
records enumeration, bounds readback, foreground-window readback and close in one
Windows boot. It explicitly used the RUN channel because the installed agent
predated the window verbs. Preserve that evidence: the integration gap above is
not a claim that the guest user32 primitives have never worked. This dated record
does not establish the current packaged product's protocol or independent host
window presentation.

## Required integration work

1. Define a shared, validated window record contract before connecting a product
   UI. Reject invalid handles/PIDs, out-of-range bounds and duplicate identities;
   never interpolate unvalidated guest strings into mutation commands.
2. Add a correlated inventory transaction to the product's running-session
   lifecycle. Publish only a complete inventory, not a partially received list.
   Define timeout, malformed record, guest restart and stale reply behavior.
   A WINEND-only delimiter does not distinguish delayed results from a new query.
3. Bind host windows to guest identities and a VM generation. Account for handle
   reuse and invalidate handles on restart. A title is not a stable identity.
4. Provide a per-window presentation path appropriate to the guest. Cropping a
   desktop framebuffer alone cannot establish correct obscured-window content.
5. Connect focus, activation, resize, close, owned dialogs and modal restrictions
   to acknowledged guest operations. Keep host Command shortcut ownership clear.
6. Add composition-aware input, candidate positioning and clipboard ownership.
   Sending an already composed Unicode string is not native IME integration.
7. Test effective DPI and multi-display geometry, minimized/obscured windows,
   guest disconnect/reset, handle reuse, rapid focus changes and stale replies.

## Evidence required before a completion claim

- Deterministic producer/consumer contracts must use the actual product paths.
- Product-session tests must cover partial, failed, delayed and duplicate replies.
- Live receipts must show real guest applications as independent host windows,
  including overlapping windows and owned dialogs, with correct content/input.
- Performance comparisons require matched guest workloads and configurations;
  protocol or parser tests cannot establish performance superiority over QEMU.

No Coherence completion, IME support, performance win or release promotion is
claimed by this document. The next implementation scope is the shared record
contract and correlated inventory path, not another isolated proxy-window demo.

## Implementation update: read-only product inventory (2026-09-11)

This update supersedes the earlier description of a missing product-side
inventory connection. It does not supersede or expand the historical live
primitive evidence, and it does not close a Coherence criterion.

Source commit `fabf2f80` connected a read-only guest-window panel to the
BridgeVMControl live display. Source commit `220235d1` subsequently bounded
each incremental product log read to 1 MiB. The implementation now includes:

- A shared validated window-record contract in
  `apps/macos/Sources/BridgeVMWindowProtocol/GuestWindowRecord.swift`.
- A host request identifier for WINLIST, retained in engine log receipts but
  stripped before delivery to the existing guest protocol. This is host-side
  correlation, not a guest-authenticated nonce or a guest lifetime identifier.
- An atomic inventory collector in
  `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowInventoryRequest.swift`.
  It publishes only after the matching WINEND and rejects malformed records,
  duplicate handles, expired requests and resource-limit violations.
- A controller that checks service readiness, clears inventory on an observed
  restart or disconnect, and a user-triggered refresh panel in
  `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowInventoryPanel.swift`.
  The panel explicitly identifies itself as read-only.

The native controller tests exercise synthetic process/log state: partial
results remain unpublished, a complete matching response is published, a
restart in the same log batch clears the result, and an unavailable guest
service prevents command submission. They do not demonstrate enumeration or
window interaction inside a running Windows guest. The bounded-reader tests
cover backlog continuation and UTF-8/line boundaries; they do not establish a
latency improvement or an aggregate memory bound for unterminated lines.

### Remaining integration work and evidence requirements

1. Validate the shipped panel against the sealed application on real Windows,
   including empty inventories, refresh, restart and service loss. A synthetic
   log receipt must not substitute for that run.
2. Establish guest-session-scoped window identity. A numeric HWND can be
   reused, and a host query UUID alone cannot make an old selection safe for
   later mutation.
3. Define and implement the window-surface delivery path, native presentation,
   ownership, dialogs and stacking. An inventory row or a cropped desktop
   rectangle is not evidence of independently presented guest windows.
4. Connect focus, move/resize and close with correlated outcomes and lifecycle
   invalidation. Preserve the previous primitive evidence as primitive
   evidence; do not describe it as proof of the current product path.
5. Complete input/IME, DPI and multi-display behavior, then exercise the
   applicable real-workload release gates at their unchanged sample counts.
6. Compare equivalent Windows workloads against QEMU with matched resources
   and recorded configurations. Neither log-read limits nor protocol unit
   tests prove a performance win.

No capability or product-state promotion follows from this update. The
registry remains the authority for user-facing capability wording. Independent
window presentation remains unconnected, and 3D acceleration remains deferred
under the Windows-first development priority.
