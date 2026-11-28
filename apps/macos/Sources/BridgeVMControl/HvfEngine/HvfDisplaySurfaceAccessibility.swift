#if canImport(AppKit)
import AppKit

/// The display forwards a click only while a guest frame is on screen, so the
/// surface tells accessibility clients whether one is presented.
enum HvfDisplaySurfaceAccessibility {
    static let identifier = "bridgevm.runtime.display.surface"
    /// The end-to-end harness launches the app with this flag and reads where its clicks went
    /// from the surface's help. VoiceOver reads help aloud, so nobody else gets the press count.
    static let reportsPresses = ProcessInfo.processInfo.arguments.contains("--e2e-library-root")
    /// What VoiceOver says after a pause on the display: where input goes, and where the VM's own controls are.
    static let help = "Keys and clicks go to Windows. Use the Virtual Machine menu to pause, restart or shut down."

    static func configure(_ view: NSView, reports: Bool = reportsPresses) {
        view.setAccessibilityElement(true)
        view.setAccessibilityRole(.image)
        view.setAccessibilityLabel("Windows display")
        view.setAccessibilityIdentifier(identifier)
        if !reports { view.setAccessibilityHelp(help) }
        update(view, guestSize: .zero)
    }

    static func update(_ view: NSView, guestSize: CGSize) {
        view.setAccessibilityValue(value(guestSize))
    }

    static func value(_ size: CGSize) -> String {
        guard size.width > 0, size.height > 0 else { return "Waiting for Windows to show a picture" }
        return "\(Int(size.width)) by \(Int(size.height)) pixels"
    }

    /// Counts the presses the view received and where the last one went, so an automated
    /// click that never reaches the guest says why.
    static func notePress(_ view: NSView, count: Int, outcome: String, reports: Bool = reportsPresses) {
        if reports { view.setAccessibilityHelp("presses=\(count) last=\(outcome)") }
    }
}
#endif
