#if canImport(AppKit)
import AppKit

/// Owns one pending pointer release; notification objects need not create windows in tests.
@MainActor
final class HvfPointerCapture: NSObject {
    private let center: NotificationCenter
    private var pendingRelease: (() -> Void)?

    init(center: NotificationCenter = .default) {
        self.center = center
        super.init()
    }

    func arm(window: AnyObject?, release: @escaping () -> Void) {
        disarm()
        pendingRelease = release
        if let window {
            center.addObserver(self, selector: #selector(cancel),
                               name: NSWindow.didResignKeyNotification, object: window)
        }
    }

    func updateRelease(_ release: @escaping () -> Void) {
        if pendingRelease != nil { pendingRelease = release }
    }

    func disarm() {
        center.removeObserver(self, name: NSWindow.didResignKeyNotification, object: nil)
        pendingRelease = nil
    }

    @objc func cancel() {
        let release = pendingRelease
        disarm()
        release?()
    }
}
#endif
