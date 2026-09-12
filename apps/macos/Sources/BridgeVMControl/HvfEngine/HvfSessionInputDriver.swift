import Foundation

@MainActor
final class HvfSessionInputDriver {
    var onPoll: (() -> Void)?
    var onDiagnostic: ((String) -> Void)?
    private var router = HvfSessionInputRouter()
    private var timer: Timer?

    deinit { timer?.invalidate() }

    func beginOwnedBoot(binding: [String]) {
        timer?.invalidate(); timer = nil
        router.beginOwnedBoot(binding: binding)
    }

    func attachUnknown(binding: [String]) {
        timer?.invalidate(); timer = nil
        router.attachUnknown(binding: binding)
    }

    /// True means handled or explicitly refused; false alone allows legacy input.
    func route(_ event: HvfOrderedInputQueue.Event, binding: [String]) -> Bool {
        switch router.route(event, binding: binding, now: Date()) {
        case .legacy: return false
        case .refused:
            onDiagnostic?("ordered input refused: unavailable, full or changed ownership")
            return true
        case .queued:
            startPumpIfNeeded()
            onPoll?()
            return true
        }
    }

    func allowLegacyWrite(binding: [String]) -> Bool { router.allowLegacyWrite(binding: binding) }

    func cancelTarget() {
        _ = router.cancelTarget()
        startPumpIfNeeded()
        onPoll?()
    }

    func poll(binding: [String], serviceReady: Bool, lines: [String], send: (String) -> Bool) {
        let previous = router.state
        let update = router.poll(binding: binding, serviceReady: serviceReady, lines: lines, now: Date(), send: send)
        if case .inserted = update {
            // Never reuse a receipt batch to acknowledge the next request.
            _ = router.poll(binding: binding, serviceReady: serviceReady, lines: [], now: Date(), send: send)
        }
        if case let .cancelled(reason, discarded) = update, discarded > 0 {
            onDiagnostic?("ordered input cancelled: \(reason), discarded=\(discarded)")
        }
        if previous != .ready, router.state == .ready {
            onDiagnostic?("ordered input active: receipts confirm insertion, not application consumption")
        }
        if router.count == 0 { timer?.invalidate(); timer = nil }
    }

    private func startPumpIfNeeded() {
        guard router.count > 0, timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.onPoll?() }
        }
    }
}
