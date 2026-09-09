import Foundation

enum BridgeVMControlLaunchPolicy {
    /// The legacy ~/.bridgevm-control import exists so the VM a user already
    /// runs shows up in *their* library. An explicit --e2e-library-root is a
    /// sealed, deliberately empty library and must stay exactly what it was
    /// given: on 2026-09-09 the import seeded the user's ubuntu-dev into a
    /// fresh E2E library, which is not the empty library the product journey
    /// is measured against.
    static func shouldMigrateLegacy(options: BridgeVMControlLaunchOptions?) -> Bool {
        options?.e2eLibraryRoot == nil
    }
}
