import SwiftUI
#if canImport(AppKit)
import AppKit
#endif
struct BridgeVMControlApp: App {
#if canImport(AppKit)
    @NSApplicationDelegateAdaptor(ControlAppDelegate.self) private var appDelegate
#endif
    @StateObject var library: BridgeVMControlAppState
    init() { _library = BridgeVMControlAppModel.libraryState() }
    var body: some Scene {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.lifecycle.record(.appBodyEvaluated)
        #endif
        return WindowGroup("BridgeVM Control") {
            BridgeVMControlAppContent(library: library)
                .frame(minWidth: 1100)
                .background(ControlLibraryWindow())
                .appUIHostSceneObservation()
        }
        .controlWindowPresentation()
        // Files opened from Finder or the Dock go to ControlAppDelegate, never to a new library window.
        .handlesExternalEvents(matching: [])
        #if canImport(AppKit)
        .commands { HvfVirtualMachineCommands() }
        #endif
    }
}
