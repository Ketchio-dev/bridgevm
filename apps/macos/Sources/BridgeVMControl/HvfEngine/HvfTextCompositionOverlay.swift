#if canImport(AppKit)
import AppKit

/// Composition is a noninteractive overlay; pointer events still target the guest.
final class HvfTextCompositionOverlay: NSTextField {
    static func make() -> HvfTextCompositionOverlay {
        let view = HvfTextCompositionOverlay(labelWithString: "")
        view.translatesAutoresizingMaskIntoConstraints = false
        view.font = .systemFont(ofSize: 18)
        view.drawsBackground = true
        view.backgroundColor = .textBackgroundColor
        view.textColor = .textColor
        view.cell?.wraps = false
        view.cell?.isScrollable = false
        view.cell?.lineBreakMode = .byClipping
        // Let the display's width clip long preedit instead of growing the view
        // to the label's intrinsic width and pushing candidates offscreen.
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.setAccessibilityIdentifier("bridgevm.runtime.keyboard.composition")
        view.setAccessibilityLabel("Composing text")
        return view
    }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
#endif
