import AppKit
import SwiftUI
@testable import BridgeVMControl

@MainActor
final class HvfAppUIWindow {
    let window: NSWindow
    private let controller: NSHostingController<ContentView>
    var content: NSView { controller.view }

    init(rootView: ContentView) {
        controller = NSHostingController(rootView: rootView)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1320, height: 860),
            styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.title = "BridgeVM — owned UI diagnostic"
        window.isReleasedWhenClosed = false
        window.contentViewController = controller
        window.contentMinSize = NSSize(width: 1100, height: 720)
        window.setContentSize(NSSize(width: 1320, height: 860))
        window.appearance = NSAppearance(named: .aqua)
        window.makeKeyAndOrderFront(nil)
    }

    func setPresentation(dark: Bool, minimum: Bool) async throws {
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.setContentSize(NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        try await Task.sleep(nanoseconds: 200_000_000)
        content.layoutSubtreeIfNeeded()
        guard abs(content.bounds.width - (minimum ? 1100 : 1320)) < 1,
              abs(content.bounds.height - (minimum ? 720 : 860)) < 1,
              content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == (dark ? .darkAqua : .aqua)
        else { throw HvfAppUIError.refused("Owned content did not adopt the requested dimensions and appearance") }
    }
}
