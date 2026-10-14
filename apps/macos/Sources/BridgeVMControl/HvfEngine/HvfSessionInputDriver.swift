import Foundation

@MainActor
final class HvfSessionInputDriver {
    var onPoll: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    private(set) var router = HvfSessionInputRouter()
    private(set) var textInputGeneration = UUID()
    private var timer: Timer?

    deinit { timer?.invalidate() }

    func beginOwnedBoot(binding: [String]) {
        textInputGeneration = UUID()
        timer?.invalidate(); timer = nil
        router.beginOwnedBoot(binding: binding)
    }

    func attachUnknown(binding: [String]) {
        textInputGeneration = UUID()
        timer?.invalidate(); timer = nil
        router.attachUnknown(binding: binding)
    }

    /// Only legacy admission allows the caller to use the legacy input path.
    func route(_ event: HvfOrderedInputQueue.Event, binding: [String]) -> HvfNegotiatedInputStream.Admission {
        let admission = router.route(event, binding: binding, now: Date())
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

    func allowLegacyWrite(binding: [String]) -> Bool { router.allowLegacyWrite(binding: binding) }

    func cancelTarget() {
        textInputGeneration = UUID()
        _ = router.cancelTarget()
        startPumpIfNeeded()
        onPoll?()
    }

    func poll(binding: [String], serviceReady: Bool, lines: [String], send: (String) -> Bool) {
        let previous = router.state
        let update = router.pollBatch(binding: binding, serviceReady: serviceReady, lines: lines, now: Date(), send: send)
        if case .inserted = update {
            // Never reuse a receipt batch to acknowledge the next request.
            _ = router.poll(binding: binding, serviceReady: serviceReady, lines: [], now: Date(), send: send)
        }
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
