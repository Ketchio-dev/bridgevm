import Foundation

/// The main run loop's busy stretches for the `app display pacing:` record: the time from each run-loop
/// activity to the next, leaving out the sleep between BeforeWaiting and AfterWaiting. A stretch over
/// 50 ms is a stall: no display-link tick, input poll or event can run behind it.
struct HvfMainThreadSpans: Equatable {
    static let stallNanoseconds: UInt64 = 50_000_000
    private(set) var stalls: UInt64 = 0
    /// The longest busy stretch in nanoseconds, stall or not.
    private(set) var longest: UInt64 = 0
    private var since: UInt64?

    /// A run-loop activity at `now` (nanoseconds); `sleeping` when the loop is about to wait.
    mutating func note(sleeping: Bool, at now: UInt64) {
        if let since, now > since {
            longest = max(longest, now - since)
            if now - since > Self.stallNanoseconds { stalls &+= 1 }
        }
        since = sleeping ? nil : now
    }
}

/// Feeds HvfMainThreadSpans from an observer on the main run loop's common modes. Main thread only:
/// the observer runs there, and start and stop are called there. It costs one clock read per activity.
final class HvfMainThreadStallObserver {
    private(set) var spans = HvfMainThreadSpans()
    private var observer: CFRunLoopObserver?
    var running: Bool { observer != nil }

    func start(clock: @escaping () -> UInt64) {
        guard observer == nil else { return }
        let observer = CFRunLoopObserverCreateWithHandler(
            kCFAllocatorDefault, CFRunLoopActivity.allActivities.rawValue, true, 0
        ) { [weak self] _, activity in
            self?.spans.note(sleeping: activity == .beforeWaiting, at: clock())
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        self.observer = observer
    }

    func stop() {
        if let observer { CFRunLoopObserverInvalidate(observer) }
        observer = nil
    }

    func reset() {
        stop()
        spans = HvfMainThreadSpans()
    }

    deinit { stop() }
}
