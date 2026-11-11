import Foundation

/// Owns one UI input channel; file emptiness is never used as drain evidence.
struct HvfSessionInputRouter {
    private(set) var binding: [String]?
    private(set) var eligible = false
    private(set) var activated = false
    private(set) var failure: HvfSessionInputFailure? // see `endsAtGuestReset` for re-arming
    var failed: Bool { failure != nil }
    private var stream = HvfRecoverableInputStream()
    var state: HvfNegotiatedInputStream.State { failed ? .failed : stream.state }
    var count: Int { stream.count }
    /// Whether a generic chord (HvfModifierChord) can reach the guest now: on the legacy route the VMM
    /// shipped with this app builds it; on the ordered route only an agent with the marker does.
    var modifierChords: Bool { !eligible || stream.modifierChords }
    /// Whether a scroll goes as raw units (`scrollunits:`) rather than `scroll:` notches.
    var scrollUnits: Bool { eligible && stream.scrollUnits }
    /// Whether a pointer action may carry held modifiers (`:<wcas>`) and whether the middle button reaches the
    /// guest: only an agent with the markers takes them; the VMM's HID path drops modifiers and has no middle button.
    var pointerModifiers: Bool { eligible && stream.pointerModifiers }
    var middleButton: Bool { eligible && stream.middleButton }
    /// Whether the agent answers `CARET` (the INPUTCAPS marker); the VMM's HID path cannot.
    var caret: Bool { eligible && stream.caret }

    mutating func beginOwnedBoot(binding: [String]) {
        self = Self()
        self.binding = binding
        eligible = true
    }

    mutating func attachUnknown(binding: [String]) {
        self = Self()
        self.binding = binding
    }

    mutating func route(_ event: HvfOrderedInputQueue.Event, binding: [String], now: Date) -> HvfNegotiatedInputStream.Admission {
        guard matches(binding), !failed else { return .refused }
        guard eligible else { return .legacy }
        let admission = stream.enqueue(event, now: now)
        if admission == .legacy {
            if activated { return .refused }
            eligible = false
            stream.reset(.targetChanged)
        }
        return admission
    }

    /// Called before every legacy write, including clipboard-backed paste.
    mutating func allowLegacyWrite(binding: [String]) -> Bool {
        guard matches(binding), !failed, !activated else { return false }
        eligible = false
        stream.reset(.targetChanged)
        return true
    }

    mutating func poll(binding: [String], serviceReady: Bool, lines: [String], now: Date,
                       send: (String) -> Bool) -> HvfAcknowledgedInputStream.Update? {
        guard matches(binding), eligible, !failed || (failure == .requestFailed && stream.isRecovering) else { return nil }
        let reset = lines.contains { HvfGuestResetRecord.matches($0) }
        if activated, let reason = HvfSessionInputFailure(reset: reset, serviceReady: serviceReady, lines: lines) {
            return fail(reason)
        }
        let update = stream.poll(serviceReady: serviceReady, legacyQuiescent: true,
                                 lines: lines, now: now, send: send)
        if stream.state == .ready { activated = true }
        if case .cancelled = update, activated { failure = .requestFailed }
        if stream.state == .unavailable {
            if activated { failure = .unavailable } else { eligible = false }
        }
        return update
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
        return stream.reset(.sessionChanged)
    }
}
