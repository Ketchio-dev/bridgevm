import Foundation

enum BridgeVMControlLaunchPolicy {
    /// The legacy ~/.bridgevm-control import exists so the VM a user already
    /// runs shows up in *their* library. An explicit --e2e-library-root is a
    /// sealed, deliberately empty library and must stay exactly what it was
    /// given: seeding the user's VMs into it would break the empty library the
    /// product journey is measured against.
    static func shouldMigrateLegacy(options: BridgeVMControlLaunchOptions?) -> Bool {
        options?.e2eLibraryRoot == nil
    }
}
