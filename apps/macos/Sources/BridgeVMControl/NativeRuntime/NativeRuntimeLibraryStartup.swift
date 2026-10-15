import Foundation

@MainActor
enum NativeRuntimeLibraryStartup {
    static func make(rootURL: URL, owner: NativeRuntimeOwner, cache: NativeRuntimeModelCache<LibraryModel>,
                     options: BridgeVMControlLaunchOptions) -> LibraryStartupCoordinator {
        let migrate = BridgeVMControlLaunchPolicy.shouldMigrateLegacy(options: options)
        return LibraryStartupCoordinator(scan: {
            try owner.validateCurrentOwnership()
            return LibraryInitialScan.perform(rootURL: rootURL, migrateLegacy: migrate)
        }, validateOwner: { try owner.validateCurrentOwnership() }, publish: { scan in
            try cache.model {
                LibraryModel(rootURL: rootURL, e2eUnattendedPath: options.e2eUnattendedPath?.path,
                    e2eHostDiagnosticStopAdmitted: options.admitsHostDiagnosticStop, initialScan: scan)
            }
        }, closeOwner: { owner.close() })
    }
}
