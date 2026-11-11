#if canImport(AppKit)
import AppKit
import SwiftUI

@MainActor
final class HvfDisplayWindowController: NSWindowController, NSWindowDelegate {
    private static var controllers: [ObjectIdentifier: HvfDisplayWindowController] = [:]

    private let sessionIdentifier: ObjectIdentifier
    private let state: HvfDisplayWindowState
    let placement: HvfDisplayWindowPlacement

    /// A new display window for the session, not shown yet: where this VM's window was last
    /// (HvfDisplayWindowPlacement), or 1280x800 centred.
    init(session: HvfEngineSession, title: String) {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: NSSize(width: 1280, height: 800)),
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
        placement = HvfDisplayWindowPlacement(config: session.config)
        placement.restore(window)
        window.collectionBehavior.insert(.fullScreenPrimary)
        window.backgroundColor = .black
        window.isReleasedWhenClosed = false

        sessionIdentifier = ObjectIdentifier(session)
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

        let controller = HvfDisplayWindowController(session: session, title: title)
        controllers[identifier] = controller
        guard let window = controller.window else { return }
        reveal(window)
        controller.placement.reenterFullScreen(window)
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

    func windowDidMove(_ notification: Notification) { if let window { placement.frameChanged(window) } }

    func windowDidResize(_ notification: Notification) { if let window { placement.frameChanged(window) } }

    func windowWillEnterFullScreen(_ notification: Notification) { placement.willEnterFullScreen() }

    func windowDidEnterFullScreen(_ notification: Notification) { placement.didEnterFullScreen() }

    func windowDidExitFullScreen(_ notification: Notification) { if let window { placement.didExitFullScreen(window) } }

    func windowDidFailToEnterFullScreen(_ window: NSWindow) { placement.didExitFullScreen(window) }

    func windowWillClose(_ notification: Notification) {
        if let window { placement.willClose(window) }
        HvfVirtualMachineMenuTarget.shared.resigned(state)
        if HvfDisplayWindowController.controllers[sessionIdentifier] === self {
            HvfDisplayWindowController.controllers[sessionIdentifier] = nil
        }
    }
}
#endif
