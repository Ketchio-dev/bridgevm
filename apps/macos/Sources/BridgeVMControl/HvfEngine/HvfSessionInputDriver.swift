import Foundation

@MainActor
final class HvfSessionInputDriver {
    var onPoll: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    /// Each admission with the queue depth before and after it, and each stream update, for the session's
    /// `app ordered input:` record (HvfOrderedInputRecord.swift); observing never changes the routing.
    var onAdmission: ((HvfNegotiatedInputStream.Admission, _ held: Int, _ depth: Int) -> Void)?
    var onUpdate: ((HvfAcknowledgedInputStream.Update?) -> Void)?
    /// Writes events the guest inserted nothing of to input.ctl, in this order, before any later input is routed.
    var onLegacyReplay: (([HvfOrderedInputQueue.Event]) -> Void)?
    /// Writes one input.ctl line the switch waits on (HvfLegacyInputActivity.barrier), after any replay.
    var onLegacyBarrier: ((String) -> Void)?
    /// The clock of the receipt deadlines; the session's leaves paused time out (HvfGuestRunClock).
    var now: () -> Date = Date.init
    private(set) var router = HvfSessionInputRouter()
    private(set) var textInputGeneration = UUID()
    private var timer: Timer?
    private var fallbackReported = false // since the last insertion
    deinit { timer?.invalidate() }
    func beginOwnedBoot(binding: [String]) { restart(); router.beginOwnedBoot(binding: binding) }
    func attachUnknown(binding: [String]) { restart(); router.attachUnknown(binding: binding) }
    private func restart() { textInputGeneration = UUID(); timer?.invalidate(); timer = nil; fallbackReported = false }
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
    /// `line` is the input.ctl line about to be written.
    func allowLegacyWrite(_ line: String, binding: [String]) -> Bool {
        let held = router.count; defer { observeBindingDiscard(held: held) }
        return router.allowLegacyWrite(line, binding: binding, now: now())
    }

    func cancelTarget() {
        textInputGeneration = UUID()
        let cancelled = router.cancelTarget(now: now())
        onUpdate?(cancelled); reportPollUpdate(cancelled)
        startPumpIfNeeded()
        onPoll?()
    }

    /// `legacyPending`: a legacy clipboard paste still has its `KEY ctrl+v` to write (HvfSessionInputRouter.poll).
    func poll(binding: [String], serviceReady: Bool, legacyPending: Bool = false, lines: [String], send: (String) -> Bool) {
        let previous = router.state, held = router.count; defer { observeBindingDiscard(held: held) }
        var update = router.pollBatch(binding: binding, serviceReady: serviceReady, lines: lines, now: now(),
                                      legacyPending: legacyPending, send: send)
        if case .inserted = update {
            onUpdate?(update)
            fallbackReported = false
            // Never reuse a receipt batch to acknowledge the next request.
            update = router.poll(binding: binding, serviceReady: serviceReady, lines: [], now: now(),
                                 legacyPending: legacyPending, send: send)
        }
        onUpdate?(update)
        if invalidatesTextInputTarget(previous: previous, lines: lines) { textInputGeneration = UUID() }
        reportPollUpdate(update, previous: previous)
        let replay = router.takeLegacyReplay()
        if !replay.isEmpty {
            if !fallbackReported {
                onDiagnostic?("ordered input fell back to the VMM keyboard and mouse: Windows inserted none of "
                              + "\(replay.count) events (a lock, sign-in or UAC screen)")
                fallbackReported = true
            }
            onLegacyReplay?(replay)
        }
        if let barrier = router.takeLegacyBarrier() { onLegacyBarrier?(barrier) }
        if router.count == 0 { timer?.invalidate(); timer = nil }
    }

    private func startPumpIfNeeded() {
        guard router.count > 0, timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.onPoll?() }
        }
    }
}
