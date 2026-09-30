#if canImport(AppKit)
import AppKit

/// The display forwards a click only while a guest frame is on screen, so the
/// surface tells accessibility clients whether one is presented.
enum HvfDisplaySurfaceAccessibility {
    static let identifier = "bridgevm.runtime.display.surface"

    static func configure(_ view: NSView) {
        view.setAccessibilityElement(true)
        view.setAccessibilityRole(.image)
        view.setAccessibilityLabel("게스트 화면")
        view.setAccessibilityIdentifier(identifier)
        update(view, guestSize: .zero)
    }

    static func update(_ view: NSView, guestSize: CGSize) {
        view.setAccessibilityValue(value(guestSize))
    }

    static func value(_ size: CGSize) -> String {
        guard size.width > 0, size.height > 0 else { return "no-frame" }
        return "frame \(Int(size.width))x\(Int(size.height))"
    }
}
#endif
