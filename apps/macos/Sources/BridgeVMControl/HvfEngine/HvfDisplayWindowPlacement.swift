#if canImport(AppKit)
import AppKit

/// Where a VM's display window was, and whether it was full screen, kept per VM in the app's defaults:
/// the frame under AppKit's frame name for the VM (a library VM by its id, the experimental entry under one
/// name of its own) and the full-screen flag beside it. A new window for the VM opens at that frame, so the
/// guest's first RESIZE asks for that content size. AppKit fits a frame saved on a screen that is gone onto
/// the screens there are, and keeps a shown window's title bar on screen. Full screen's frame is never saved,
/// only the window's own before and after it.
@MainActor
final class HvfDisplayWindowPlacement {
    let frameName: String
    var fullScreenKey: String { frameName + " full screen" }
    /// From entering full screen until it is left again.
    private var inFullScreen = false
    /// A closing window may still leave full screen; the state it was closed in is the one to keep.
    private var closed = false

    init(config: HvfEngineConfig) {
        frameName = "BridgeVM display " + (config.libraryContext.map { "vm " + $0.config.slug } ?? "experimental")
    }

    /// Whether the VM's window was full screen when it was last closed or the app quit.
    var fullScreen: Bool { UserDefaults.standard.bool(forKey: fullScreenKey) }

    /// The saved frame, if there is one; the window keeps the frame it has otherwise.
    func restore(_ window: NSWindow) {
        window.setFrameUsingName(frameName)
    }

    /// Once the window is shown: back to full screen if it was.
    func reenterFullScreen(_ window: NSWindow) {
        guard fullScreen else { return }
        DispatchQueue.main.async { [weak window] in
            guard let window, window.isVisible, !window.styleMask.contains(.fullScreen) else { return }
            window.toggleFullScreen(nil)
        }
    }

    func frameChanged(_ window: NSWindow) {
        guard !inFullScreen, !closed else { return }
        window.saveFrame(usingName: frameName)
    }

    func willEnterFullScreen() { inFullScreen = true }

    func didEnterFullScreen() {
        guard !closed else { return }
        UserDefaults.standard.set(true, forKey: fullScreenKey)
    }

    /// Left full screen, or failed to enter it.
    func didExitFullScreen(_ window: NSWindow) {
        inFullScreen = false
        guard !closed else { return }
        UserDefaults.standard.set(false, forKey: fullScreenKey)
        frameChanged(window)
    }

    func willClose(_ window: NSWindow) {
        frameChanged(window)
        closed = true
    }
}
#endif
