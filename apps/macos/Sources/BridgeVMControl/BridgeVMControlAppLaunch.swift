import Foundation

enum BridgeVMControlAppLaunch {
    @MainActor static func libraryModel() -> LibraryModel {
        guard let owner = NativeRuntimeAppOwner.prepared else {
            preconditionFailure("Native library owner must be admitted before model creation")
        }
        return owner.model()
    }
}
