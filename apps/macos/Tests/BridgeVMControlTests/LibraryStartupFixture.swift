import Foundation
@testable import BridgeVMControl

@MainActor
final class LibraryStartupFixture {
    let recovery: LibraryStartupRecoveryFixture
    let library: NativeRuntimeLibraryHandle
    let owner: NativeRuntimeOwner
    let cache: NativeRuntimeModelCache<LibraryModel>
    private(set) var publications = 0
    private(set) var closes = 0

    init() throws {
        recovery = try LibraryStartupRecoveryFixture()
        library = try NativeRuntimeLibraryHandle.open(rootURL: recovery.library, create: false)
        owner = try NativeRuntimeOwner(library: library)
        cache = NativeRuntimeModelCache(owner: owner)
    }
    func coordinator(probe: LibraryStartupProbe) -> LibraryStartupCoordinator {
        let root = recovery.library
        return LibraryStartupCoordinator(scan: { try probe.scan(rootURL: root) },
            validateOwner: { try self.owner.validateCurrentOwnership() }, publish: { scan in
                self.publications += 1
                return try self.cache.model {
                    LibraryModel(rootURL: root, migrateLegacy: false, initialScan: scan,
                        startsModelsAutomatically: false)
                }
            }, closeOwner: { self.closes += 1; self.owner.close() })
    }
    func clean() {
        owner.close()
        try? FileManager.default.removeItem(atPath: owner.endpoint.directoryPath)
        recovery.clean()
    }
}
