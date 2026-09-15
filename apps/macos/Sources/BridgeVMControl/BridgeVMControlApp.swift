import SwiftUI
#if canImport(AppKit)
import AppKit
/// Ensure the window appears and takes focus when launched as a SwiftPM
/// executable (no .app bundle), rather than starting as a background agent.
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.applicationDidFinishLaunching(notification)
        #endif
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
#endif
struct BridgeVMControlApp: App {
#if canImport(AppKit)
    @NSApplicationDelegateAdaptor(ControlAppDelegate.self) private var appDelegate
#endif
    @StateObject var library: LibraryModel
    init() { _library = BridgeVMControlAppModel.libraryState() }
    var body: some Scene {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.lifecycle.record(.appBodyEvaluated)
        #endif
        return WindowGroup("BridgeVM Control") {
            ContentView(library: library)
                .frame(minWidth: 1100, minHeight: 720)
                .appUIHostSceneObservation()
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 1320, height: 860)
    }
}
