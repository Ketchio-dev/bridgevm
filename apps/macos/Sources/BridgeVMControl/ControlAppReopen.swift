#if canImport(AppKit)
import AppKit
import SwiftUI

struct ControlAppReopenPlan: Equatable {
    let showLibrary: Bool
    let displayIndices: [Int]

    static func make(hasVisibleWindows: Bool, runningVMs: [Bool], visibleDisplays: [Bool]) -> Self {
        guard !hasVisibleWindows else { return .init(showLibrary: false, displayIndices: []) }
        return .init(showLibrary: true, displayIndices: runningVMs.indices.filter {
            runningVMs[$0] && !visibleDisplays[$0]
        })
    }
}

/// Keep the SwiftUI library window available after its close button, without creating a second model.
@MainActor
struct ControlLibraryWindow: NSViewRepresentable {
    static var window: NSWindow?
    final class Observer: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window {
                window.isReleasedWhenClosed = false
                ControlLibraryWindow.window = window
            }
        }
    }
    func makeNSView(context: Context) -> Observer { Observer() }
    func updateNSView(_ view: Observer, context: Context) {}

    static func show() {
        guard let window else { return }
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
    }
}
#endif
