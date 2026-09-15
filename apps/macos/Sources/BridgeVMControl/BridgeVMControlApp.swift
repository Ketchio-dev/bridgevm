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
        AppUIHostLaunchControl.recordBody()
        #endif
        return WindowGroup("BridgeVM Control") {
            #if DEBUG && BRIDGEVM_APP_UI_HOST && BRIDGEVM_APP_UI_LAUNCH_CONTROL
            Text("BridgeVM launch control")
                .frame(minWidth: 1100, minHeight: 720)
                .appUIHostSceneObservation()
            #else
            ContentView(library: library)
                .frame(minWidth: 1100, minHeight: 720)
                .appUIHostSceneObservation()
            #endif
        }
        .controlWindowPresentation()
    }
}
