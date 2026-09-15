import SwiftUI

enum BridgeVMControlAppModel {
    @MainActor static func libraryState() -> StateObject<LibraryModel> {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        return AppUIHostLifecycle.libraryState()
        #else
        return StateObject(wrappedValue: BridgeVMControlAppLaunch.libraryModel())
        #endif
    }
}
