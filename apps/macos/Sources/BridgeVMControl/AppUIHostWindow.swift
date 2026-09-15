#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import SwiftUI

@MainActor
struct AppUIHostWindow: NSViewRepresentable {
    final class AttachmentView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window { AppUIHost.prepared?.attach(window: window) }
        }
    }
    func makeNSView(context: Context) -> NSView { AttachmentView(frame: .zero) }
    func updateNSView(_ nsView: NSView, context: Context) {}

    @MainActor static func setPresentation(_ window: NSWindow, dark: Bool, minimum: Bool) async throws {
        guard let content = window.contentView, window.isVisible else {
            throw AppUIHostError.refused("Actual app window has no visible owned content")
        }
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.setContentSize(NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        try await Task.sleep(nanoseconds: 200_000_000)
        content.layoutSubtreeIfNeeded()
        guard window.contentView === content,
              abs(content.bounds.width - (minimum ? 1100 : 1320)) < 1,
              abs(content.bounds.height - (minimum ? 720 : 860)) < 1,
              content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == (dark ? .darkAqua : .aqua)
        else { throw AppUIHostError.refused("Actual content did not adopt requested dimensions and appearance") }
    }
}
#endif
