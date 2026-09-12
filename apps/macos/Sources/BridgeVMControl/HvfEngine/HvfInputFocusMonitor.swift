#if canImport(AppKit)
import AppKit

@MainActor
final class HvfInputFocusMonitor {
    private var observer: NSObjectProtocol?

    func watch(window: NSWindow?, onResign: @escaping @MainActor () -> Void) {
        stop()
        guard let window else { return }
        observer = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification,
                                                          object: window, queue: .main) { _ in
            Task { @MainActor in onResign() }
        }
    }

    func stop() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
    }

    deinit { if let observer { NotificationCenter.default.removeObserver(observer) } }
}
#endif
