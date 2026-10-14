import Foundation

enum BridgeVMControlAppLaunch {
    @MainActor static func libraryStartup() -> LibraryStartupCoordinator {
        guard let owner = NativeRuntimeAppOwner.prepared else {
            preconditionFailure("Native library owner must be admitted before model creation")
        }
        return owner.libraryStartup()
    }
}
