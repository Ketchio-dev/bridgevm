#if canImport(AppKit)
import AppKit
/// Ensure the window appears and takes focus when launched as a SwiftPM
/// executable (no .app bundle), rather than starting as a background agent.
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        ControlAppActivation.activate()
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.applicationDidFinishLaunching(notification)
        #endif
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
#endif
