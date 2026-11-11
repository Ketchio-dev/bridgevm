import Foundation

/// Owns one UI input channel; file emptiness is never used as drain evidence. Input goes to input.ctl, the VMM's
/// HID route, until the agent's ordered route is ready and input.ctl is quiet (HvfLegacyInputActivity); an owned
/// boot then activates. After activation the router uses input.ctl again only in `fallback`: when the guest
/// inserted nothing of an event (a Windows secure desktop), that event and everything queued behind it are handed
/// back for input.ctl in their original order (`takeLegacyReplay`), later input follows them there, and once
/// input.ctl is quiet again (and Windows inserted the release an ordered press still owes) the next event tries
/// the ordered route. Ctrl+Alt+Delete always takes the VMM keyboard, and on an active route nothing overtakes it
/// there: input after it follows it on input.ctl (`fallback`), and behind ordered input still going in it waits,
/// with input after it, until that input has gone in (`takeSecureAttention`). An agent restart within the boot
/// (re-READY, a new SERVICE start) starts the owned boot over: the new agent instance negotiates again
/// (HvfSessionInputGeneration.swift). A held key (`down:<key>` until `up:<key>`, HvfModifierChord.down) goes up on the
/// route it went down (`routeKeyUp`): on input.ctl while the VMM holds it, which also keeps the switch waiting; on the
/// ordered route, or through its cleanup (HvfPointerCleanup) like a held button.
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
    /// Whether a key can be held from its press to its release now: on the legacy route the VMM shipped with this app
    /// holds it; on the ordered route only an agent with the HOLD marker. Never behind a Ctrl+Alt+Delete: input there
    /// reaches input.ctl later, after its release could have gone.
    var keyHold: Bool { held.isEmpty && !failed && (!eligible || fallback || stream.keyHold) }
    /// The keys input.ctl holds down (HvfLegacyInputActivity): their release goes there too.
    var legacyHeldKeys: [String] { legacy.keys }

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

    /// A held key goes up where it went down: `.legacy` while input.ctl holds it (whatever the state), `.queued` behind
    /// its press on an ordered route taking input (even a full queue takes it), and `.refused` when neither has anything
    /// to send now (a press the ordered route still owes a release for goes up through its cleanup: leaving a
    /// fallback, a request failure or the next target change).
    mutating func routeKeyUp(_ action: String, binding: [String], now: Date) -> HvfNegotiatedInputStream.Admission {
        guard matches(binding) else { return .refused }
        if legacy.keys.contains(action) { return .legacy }
        guard eligible, activated, !failed, !fallback, held.isEmpty, stream.state == .ready else { return .refused }
        return stream.enqueue(.key(HvfModifierChord.up + action), now: now)
    }

    /// Called with the line before every input.ctl write of guest input, clipboard-backed paste included.
    mutating func allowLegacyWrite(_ line: String, binding: [String], now: Date) -> Bool {
        guard matches(binding) else { return false }
        let secureAttention = line.hasPrefix("KEY ") && HvfGuestInputEncoding.isSecureAttention(String(line.dropFirst(4)))
        // A key the VMM holds always goes up there, even once the router failed: nothing else would release it.
        let release = line.hasPrefix("KEY " + HvfModifierChord.up)
            && legacy.keys.contains(HvfModifierChord.heldAction(String(line.dropFirst(4))) ?? "")
        guard secureAttention || release || (!failed && (!activated || fallback)) else { return false }
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
            // Windows keeps a key the old agent instance pressed down (SendInput's key state is not per process), and
            // the new instance knows nothing of it: its release goes to the new instance now, ahead of the next
            // INPUTCAPS. Nothing waits for its receipt (an id no later request takes); one that inserts nothing is lost.
            if reason == .agentRestarted {
                for key in stream.heldKeys {
                    if let request = HvfUnicodeInputRequest(event: .key(HvfModifierChord.up + key), now: now) { _ = send(request.command) }
                }
            }
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
        // A boot that never activated stays owned and legacy: the stream asks the next agent instance again.
        if stream.state == .unavailable, activated { failure = .unavailable }
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

    /// A changed target discards what waits for the old one: the stream's queue, and input held behind a
    /// Ctrl+Alt+Delete, which must never reach the guest later.
    mutating func cancelTarget(now: Date = Date()) -> HvfAcknowledgedInputStream.Update {
        let dropped = held.count + heldOut.count
        held.removeAll(); heldOut.removeAll()
        let update = stream.reset(.targetChanged, now: now)
        guard dropped > 0 else { return update }
        if case let .cancelled(reason, discarded) = update { return .cancelled(reason, discarded: discarded + dropped) }
        return .cancelled(.targetChanged, discarded: dropped)
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
