import AppKit
import SwiftUI

/// Native sheet close requests use the same validation as Done and Escape.
struct HvfSettingsDismissGuard: NSViewRepresentable {
    let shouldDismiss: () -> Bool
    func makeCoordinator() -> Coordinator { Coordinator(shouldDismiss: shouldDismiss) }
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { context.coordinator.attach(view.window) }
        return view
    }
    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.shouldDismiss = shouldDismiss
        DispatchQueue.main.async { context.coordinator.attach(view.window) }
    }
    static func dismantleNSView(_ view: NSView, coordinator: Coordinator) { coordinator.detach() }

    final class Coordinator: NSObject, NSWindowDelegate {
        var shouldDismiss: () -> Bool
        weak var window: NSWindow?
        weak var previous: NSWindowDelegate?
        init(shouldDismiss: @escaping () -> Bool) { self.shouldDismiss = shouldDismiss }
        func attach(_ window: NSWindow?) {
            guard let window, self.window !== window else { return }
            detach()
            self.window = window
            previous = window.delegate
            window.delegate = self
        }
        func detach() {
            if window?.delegate === self { window?.delegate = previous }
            window = nil
        }
        func windowShouldClose(_ sender: NSWindow) -> Bool { shouldDismiss() }
        override func responds(to selector: Selector!) -> Bool {
            super.responds(to: selector) || previous?.responds(to: selector) == true
        }
        override func forwardingTarget(for selector: Selector!) -> Any? {
            previous?.responds(to: selector) == true ? previous : super.forwardingTarget(for: selector)
        }
    }
}
