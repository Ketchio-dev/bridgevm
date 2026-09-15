import Foundation

enum BridgeVMControlAppModel {
    @MainActor static func libraryModel() -> LibraryModel {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        guard let host = AppUIHost.prepared else { fatalError("Diagnostic host was not prepared") }
        return host.libraryForApplication()
        #else
        return BridgeVMControlAppLaunch.libraryModel()
        #endif
    }
}
