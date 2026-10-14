import SwiftUI

#if DEBUG && BRIDGEVM_APP_UI_HOST
typealias BridgeVMControlAppState = LibraryModel
#else
typealias BridgeVMControlAppState = LibraryStartupCoordinator
#endif

struct BridgeVMControlAppContent: View {
    @ObservedObject var library: BridgeVMControlAppState
    var body: some View {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        ContentView(library: library)
        #else
        LibraryStartupView(startup: library)
        #endif
    }
}
