#if canImport(AppKit)
import AppKit
@MainActor
final class HvfInputFocusMonitor {
    private var observer: NSObjectProtocol?
    private var generation = UUID()
    func watch(window: NSWindow?, onResign: @escaping @MainActor () -> Void) {
        stop()
        guard let window else { return }
        let expected = generation
        observer = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification,
                                                          object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { // Delivery is already on .main; do not defer target invalidation.
                guard self?.generation == expected else { return }; onResign()
            }
        }
    }
    func stop() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil; generation = UUID()
    }
    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }
}
#endif
