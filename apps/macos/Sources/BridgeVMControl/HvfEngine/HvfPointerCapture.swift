#if canImport(AppKit)
import AppKit

/// Owns each pressed button's release; notification objects need not create windows in tests.
@MainActor
final class HvfPointerCapture: NSObject {
    private let center: NotificationCenter
    private var pendingReleases: [Int: () -> Void] = [:]

    init(center: NotificationCenter = .default) {
        self.center = center
        super.init()
    }

    func arm(window: AnyObject?, button: Int = 0, release: @escaping () -> Void) {
        center.removeObserver(self, name: NSWindow.didResignKeyNotification, object: nil)
        pendingReleases[button] = release
        if let window {
            center.addObserver(self, selector: #selector(cancel),
                               name: NSWindow.didResignKeyNotification, object: window)
        }
    }

    func isArmed(button: Int) -> Bool { pendingReleases[button] != nil }

    func updateRelease(_ release: @escaping () -> Void, button: Int = 0) {
        if pendingReleases[button] != nil { pendingReleases[button] = release }
    }

    func disarm(button: Int? = nil) {
        if let button { pendingReleases.removeValue(forKey: button) }
        else { pendingReleases.removeAll() }
        if pendingReleases.isEmpty {
            center.removeObserver(self, name: NSWindow.didResignKeyNotification, object: nil)
        }
    }

    @objc func cancel() {
        let releases = pendingReleases.sorted { $0.key < $1.key }.map(\.value)
        disarm()
        for release in releases { release() }
    }
}
#endif
