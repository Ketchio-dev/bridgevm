#if canImport(AppKit)
import AppKit
import SwiftUI

@MainActor
final class HvfDisplayWindowController: NSWindowController, NSWindowDelegate {
    private static var controllers: [ObjectIdentifier: HvfDisplayWindowController] = [:]

    private let sessionIdentifier: ObjectIdentifier
    private let state: HvfDisplayWindowState

    private init(window: NSWindow, sessionIdentifier: ObjectIdentifier, state: HvfDisplayWindowState) {
        self.sessionIdentifier = sessionIdentifier
        self.state = state
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
            reveal(window)
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
        let state = HvfDisplayWindowState(session: session)
        state.window = window
        let hostingController = NSHostingController(
            rootView: HvfLiveDisplaySurface(session: session, window: state)
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
            sessionIdentifier: identifier,
            state: state
        )
        controllers[identifier] = controller

        reveal(window)
    }

    /// `makeKeyAndOrderFront` can leave an already-open auxiliary window behind
    /// the SwiftUI dashboard when the action originates in that dashboard.  The
    /// display is explicitly user-requested, so restore it and raise it even when
    /// it was minimized or belongs to another app-window ordering group.
    private static func reveal(_ window: NSWindow) {
        if window.isMiniaturized { window.deminiaturize(nil) }
        NSApp.activate(ignoringOtherApps: true)
        window.orderFrontRegardless()
        window.makeKey()
        window.makeMain() // the 가상 머신 menu acts on the main VM window
    }

    func windowDidBecomeMain(_ notification: Notification) { HvfVirtualMachineMenuTarget.shared.became(state) }

    func windowDidResignMain(_ notification: Notification) { HvfVirtualMachineMenuTarget.shared.resigned(state) }

    func windowWillClose(_ notification: Notification) {
        HvfVirtualMachineMenuTarget.shared.resigned(state)
        if HvfDisplayWindowController.controllers[sessionIdentifier] === self {
            HvfDisplayWindowController.controllers[sessionIdentifier] = nil
        }
    }
}
#endif
