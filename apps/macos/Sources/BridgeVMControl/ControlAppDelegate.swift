#if canImport(AppKit)
import AppKit
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        ControlAppActivation.activate()
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.applicationDidFinishLaunching(notification)
        #endif
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { NativeRuntimeAppOwner.shutdown() }
}
#endif
