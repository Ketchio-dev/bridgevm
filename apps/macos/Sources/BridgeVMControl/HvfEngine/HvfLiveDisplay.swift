#if canImport(AppKit)
import AppKit
import SwiftUI

struct HvfLiveDisplaySurface: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        HvfFramebufferView(session: session)
            .background(Color.black)
    }
}

@MainActor
final class HvfDisplayWindowController: NSWindowController, NSWindowDelegate {
    private static var controllers: [ObjectIdentifier: HvfDisplayWindowController] = [:]

    private let sessionIdentifier: ObjectIdentifier

    private init(window: NSWindow, sessionIdentifier: ObjectIdentifier) {
        self.sessionIdentifier = sessionIdentifier
        super.init(window: window)
        window.delegate = self
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    static func present(session: HvfEngineSession, title: String) {
        let identifier = ObjectIdentifier(session)

        if let controller = controllers[identifier], let window = controller.window {
            window.title = title
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }

        let contentRect = NSRect(origin: .zero, size: NSSize(width: 1280, height: 800))
        let window = NSWindow(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = title
        let hostingController = NSHostingController(
            rootView: HvfLiveDisplaySurface(session: session)
                .frame(minWidth: 640, minHeight: 400)
        )
        if #available(macOS 13.0, *) {
            hostingController.sizingOptions = []
        }
        window.contentViewController = hostingController
        window.setContentSize(NSSize(width: 1280, height: 800))
        window.center()
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.backgroundColor = .black
        window.isReleasedWhenClosed = false

        let controller = HvfDisplayWindowController(
            window: window,
            sessionIdentifier: identifier
        )
        controllers[identifier] = controller

        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        if HvfDisplayWindowController.controllers[sessionIdentifier] === self {
            HvfDisplayWindowController.controllers[sessionIdentifier] = nil
        }
    }
}
#endif
