import Foundation

/// Owns one UI input channel; file emptiness is never used as drain evidence. Input goes to input.ctl, the VMM's
/// HID route, until the agent's ordered route is ready and input.ctl is quiet (HvfLegacyInputActivity); an owned
/// boot then activates. After activation the router uses input.ctl again only in `fallback`: when the guest
/// inserted nothing of an event (a Windows secure desktop), that event and everything queued behind it are handed
/// back for input.ctl in their original order (`takeLegacyReplay`), later input follows them there, and once
/// input.ctl is quiet again (and Windows inserted the release an ordered press still owes) the next event tries
/// the ordered route. Ctrl+Alt+Delete always takes the VMM keyboard, and on an active route nothing overtakes it
/// there: input after it follows it on input.ctl (`fallback`), and behind ordered input still going in it waits,
/// with input after it, until that input has gone in (`takeSecureAttention`).
struct HvfSessionInputRouter {
    private(set) var binding: [String]?
    private(set) var eligible = false
    private(set) var activated = false
    /// An activated router writing input to input.ctl; the ordered queue is empty whenever this is set.
    private(set) var fallback = false
    private(set) var failure: HvfSessionInputFailure? // see `endsAtGuestReset` for re-arming
    var failed: Bool { failure != nil }
    private var stream = HvfRecoverableInputStream()
    private var legacy = HvfLegacyInputActivity()
    private var replay: [HvfOrderedInputQueue.Event] = []
    private var barrier: String?
    /// Ctrl+Alt+Delete routed behind ordered input still going in, and input routed after it; then, once that input
    /// has gone in, the same events for input.ctl.
    private var held: [HvfOrderedInputQueue.Event] = [], heldOut: [HvfOrderedInputQueue.Event] = []
    var state: HvfNegotiatedInputStream.State { failed ? .failed : stream.state }
    var count: Int { stream.count + held.count }
    /// Whether a generic chord (HvfModifierChord) can reach the guest now: on the legacy route the VMM
    /// shipped with this app builds it; on the ordered route only an agent with the marker does.
    var modifierChords: Bool { !eligible || fallback || stream.modifierChords }
    /// Whether a scroll goes as raw units (`scrollunits:`) rather than `scroll:` notches.
    var scrollUnits: Bool { eligible && !fallback && stream.scrollUnits }
    /// Whether a pointer action may carry held modifiers (`:<wcas>`) and whether the middle button reaches the
    /// guest: only an agent with the markers takes them; the VMM's HID path drops modifiers and has no middle button.
    var pointerModifiers: Bool { eligible && !fallback && stream.pointerModifiers }
    var middleButton: Bool { eligible && !fallback && stream.middleButton }
    /// Whether the agent answers `CARET` (the INPUTCAPS marker); the VMM's HID path cannot, and on a secure
    /// desktop (fallback) the agent sees only the user desktop's caret.
    var caret: Bool { eligible && !fallback && stream.caret }

    mutating func beginOwnedBoot(binding: [String]) {
        self = Self()
        self.binding = binding
        eligible = true
    }

    /// A guest reset's next generation: a fresh owned boot, but what input.ctl still owes carries over (the VMM's
    /// queue survives an in-process reboot; HvfLegacyInputActivity.observe ends it at a process recreation).
    mutating func beginNextOwnedBoot(binding: [String]) {
        let legacy = self.legacy
        beginOwnedBoot(binding: binding)
        self.legacy = legacy
    }

    mutating func attachUnknown(binding: [String]) {
        self = Self()
        self.binding = binding
    }

    mutating func route(_ event: HvfOrderedInputQueue.Event, binding: [String], now: Date) -> HvfNegotiatedInputStream.Admission {
        guard matches(binding) else { return .refused }
        var secureAttention = false
        if case let .key(key) = event { secureAttention = HvfGuestInputEncoding.isSecureAttention(key) }
        if !held.isEmpty || (secureAttention && activated && !failed && !fallback && stream.count > 0) {
            if event.isPointerMove, held.last?.isPointerMove == true { held.removeLast() } // a newer move supersedes it
            guard held.count < HvfOrderedInputQueue.maximumEvents else { return .refused }
            held.append(event)
            return .queued
        }
        if secureAttention { return .legacy }
        guard !failed else { return .refused }
        guard eligible, !fallback else { return .legacy }
        let admission = stream.enqueue(event, now: now)
        return admission == .legacy && activated ? .refused : admission
    }

    /// Called with the line before every input.ctl write of guest input, clipboard-backed paste included.
    mutating func allowLegacyWrite(_ line: String, binding: [String], now: Date) -> Bool {
        guard matches(binding) else { return false }
        let secureAttention = line.hasPrefix("KEY ") && HvfGuestInputEncoding.isSecureAttention(String(line.dropFirst(4)))
        guard secureAttention || (!failed && (!activated || fallback)) else { return false }
        // Input after the chord follows it on input.ctl (Windows may switch desktops only once the VMM's keyboard
        // queue moves), until the VMM has taken it and input.ctl is quiet.
        if secureAttention, activated, !failed, stream.count == 0, stream.state == .ready { fallback = true }
        legacy.wrote(line, now: now)
        return true
    }

    /// `legacyPending`: a legacy clipboard paste still has its `KEY ctrl+v` to write, so input.ctl is not quiet.
    mutating func poll(binding: [String], serviceReady: Bool, lines: [String], now: Date, legacyPending: Bool = false,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        legacy.observe(lines, now: now)
        guard matches(binding), eligible, !failed || (failure == .requestFailed && stream.isRecovering) else { return nil }
        let reset = lines.contains { HvfGuestResetRecord.matches($0) }
        if activated, let reason = HvfSessionInputFailure(reset: reset, serviceReady: serviceReady, lines: lines) {
            return fail(reason)
        }
        let quiet = !legacyPending && legacy.quiescent(now: now)
        // An ordered press Windows inserted may still be held (its release inserted nothing, and the HID re-send
        // cannot release a SendInput press): leaving the fallback sends that release first, the HID route stays
        // until Windows inserted it, and one that inserts nothing again is retried after the next quiet second.
        if fallback, quiet, !stream.isRecovering, stream.owesRelease { stream.releaseOwed(now: now) }
        var update = stream.poll(serviceReady: serviceReady, legacyQuiescent: quiet,
                                 lines: lines, now: now, send: send)
        if fallback, quiet, !stream.isRecovering, !stream.owesRelease { fallback = false }
        if stream.state == .ready { activated = true }
        if case let .cancelled(reason, _) = update, activated {
            if reason == .notInserted {
                // Not discarded: the caller writes them to input.ctl (takeLegacyReplay).
                replay += stream.takeNotInserted()
                fallback = true
                legacy.restart(now: now)
                update = .cancelled(.notInserted, discarded: 0)
            } else {
                failure = .requestFailed
            }
        }
        if stream.state == .unavailable {
            if activated { failure = .unavailable } else { eligible = false }
        }
        if !held.isEmpty, stream.count == 0 { // the ordered input ahead of the chord has gone in or was handed back
            if activated, !failed, stream.state == .ready { fallback = true; legacy.restart(now: now) }
            heldOut += held
            held.removeAll()
        }
        // A switch that waits on input.ctl after a move gets the move's barrier line (HvfLegacyInputActivity.barrier).
        let waiting = eligible && !failed && (fallback || stream.state == .waitingForLegacy)
        barrier = waiting && !legacyPending ? legacy.barrier(now: now) : nil
        return update
    }

    /// The line to write to input.ctl now, through the session's legacy write path like any other.
    mutating func takeLegacyBarrier() -> String? {
        defer { barrier = nil }
        return barrier
    }

    /// Ctrl+Alt+Delete and the input routed after it, once the ordered input ahead of it has gone in: for input.ctl
    /// in this order, after any `takeLegacyReplay`, before any later input.
    mutating func takeSecureAttention() -> [HvfOrderedInputQueue.Event] {
        defer { heldOut.removeAll() }
        return heldOut
    }

    /// The events of the last fallback, for input.ctl in this order before any later input.
    mutating func takeLegacyReplay() -> [HvfOrderedInputQueue.Event] {
        defer { replay.removeAll() }
        return replay
    }

    mutating func cancelTarget(now: Date = Date()) -> HvfAcknowledgedInputStream.Update {
        stream.reset(.targetChanged, now: now)
    }

    private mutating func matches(_ next: [String]) -> Bool {
        guard binding == nil || binding == next else {
            _ = fail(.bindingChanged)
            return false
        }
        return true
    }

    private mutating func fail(_ reason: HvfSessionInputFailure) -> HvfAcknowledgedInputStream.Update {
        failure = reason
        eligible = false
        fallback = false
        replay.removeAll()
        barrier = nil
        let dropped = held.count
        held.removeAll(); heldOut.removeAll()
        let update = stream.reset(.sessionChanged)
        guard dropped > 0, case let .cancelled(reason, discarded) = update else { return update }
        return .cancelled(reason, discarded: discarded + dropped)
    }
}
