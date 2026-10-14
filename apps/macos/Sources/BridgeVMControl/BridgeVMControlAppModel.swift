import SwiftUI

enum BridgeVMControlAppModel {
    @MainActor static func libraryState() -> StateObject<BridgeVMControlAppState> {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        return AppUIHostLifecycle.libraryState()
        #else
        return StateObject(wrappedValue: BridgeVMControlAppLaunch.libraryStartup())
        #endif
    }
}
