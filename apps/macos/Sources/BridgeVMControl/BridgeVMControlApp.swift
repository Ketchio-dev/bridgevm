import SwiftUI
#if canImport(AppKit)
import AppKit
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
                .frame(minWidth: 1100)
                .appUIHostSceneObservation()
        }
        .controlWindowPresentation()
    }
}
