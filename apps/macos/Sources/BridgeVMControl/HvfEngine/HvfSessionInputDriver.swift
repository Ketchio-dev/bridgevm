import Foundation

@MainActor
final class HvfSessionInputDriver {
    var onPoll: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    /// Each admission with the queue depth before and after it, and each stream update, for the session's
    /// `app ordered input:` record (HvfOrderedInputRecord.swift); observing never changes the routing.
    var onAdmission: ((HvfNegotiatedInputStream.Admission, _ held: Int, _ depth: Int) -> Void)?
    var onUpdate: ((HvfAcknowledgedInputStream.Update?) -> Void)?
    /// The clock of the receipt deadlines; the session's leaves paused time out (HvfGuestRunClock).
    var now: () -> Date = Date.init
    private(set) var router = HvfSessionInputRouter()
    private(set) var textInputGeneration = UUID()
    private var timer: Timer?
    deinit { timer?.invalidate() }
    func beginOwnedBoot(binding: [String]) { restart(); router.beginOwnedBoot(binding: binding) }
    func attachUnknown(binding: [String]) { restart(); router.attachUnknown(binding: binding) }
    private func restart() { textInputGeneration = UUID(); timer?.invalidate(); timer = nil }
    /// Only legacy admission allows the caller to use the legacy input path.
    func route(_ event: HvfOrderedInputQueue.Event, binding: [String]) -> HvfNegotiatedInputStream.Admission {
        let held = router.count; defer { observeBindingDiscard(held: held) }
        let admission = router.route(event, binding: binding, now: now())
        onAdmission?(admission, held, router.count)
        switch admission {
        case .legacy: break
        case .refused:
            onDiagnostic?("ordered input refused: unavailable, full or changed ownership")
        case .queued:
            startPumpIfNeeded()
            onPoll?()
        }
        return admission
    }
    func allowLegacyWrite(binding: [String]) -> Bool {
        let held = router.count; defer { observeBindingDiscard(held: held) }
        return router.allowLegacyWrite(binding: binding)
    }

    func cancelTarget() {
        textInputGeneration = UUID()
        let cancelled = router.cancelTarget(now: now())
        onUpdate?(cancelled); reportPollUpdate(cancelled)
        startPumpIfNeeded()
        onPoll?()
    }

    func poll(binding: [String], serviceReady: Bool, lines: [String], send: (String) -> Bool) {
        let previous = router.state, held = router.count; defer { observeBindingDiscard(held: held) }
        var update = router.pollBatch(binding: binding, serviceReady: serviceReady, lines: lines, now: now(), send: send)
        if case .inserted = update {
            onUpdate?(update)
            // Never reuse a receipt batch to acknowledge the next request.
            update = router.poll(binding: binding, serviceReady: serviceReady, lines: [], now: now(), send: send)
        }
        onUpdate?(update)
        if invalidatesTextInputTarget(previous: previous, lines: lines) { textInputGeneration = UUID() }
        reportPollUpdate(update, previous: previous)
        if router.count == 0 { timer?.invalidate(); timer = nil }
    }

    private func startPumpIfNeeded() {
        guard router.count > 0, timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.onPoll?() }
        }
    }
}
