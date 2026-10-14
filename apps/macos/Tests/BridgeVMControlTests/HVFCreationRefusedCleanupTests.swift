import XCTest
@testable import BridgeVMControl

final class HVFCreationRefusedCleanupTests: XCTestCase {
    func testMarkerFreePublicationFailureStillRollsBackBothHVFFactories() throws {
        let source = try FirstRunImportTransactionFixture()
        defer { source.remove() }
        for separateStorage in [true, false] {
            for importing in [true, false] {
                let fixture = try HVFCreationPublicationFixture(separateStorage: separateStorage)
                defer { fixture.remove() }
                let refuse: (VMConfig, URL) -> VMRegistrationCommitOutcome = { config, _ in
                    fixture.capturedConfig = config
                    return .notPublished(CocoaError(.fileWriteUnknown))
                }
                let outcome = importing
                    ? VMLibrary.createWindowsHVF(name: "Refused Import", targetDiskPath: source.disk.path,
                        varsPath: source.vars.path, storageDir: fixture.storage, libraryRoot: fixture.library,
                        snapshotHelper: HvfMediaImportTestSupport.helper, publish: refuse)
                    : VMLibrary.createWindowsHVFInstall(name: "Refused Install", isoPath: source.disk.path,
                        diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
                        storageDir: fixture.storage, libraryRoot: fixture.library, publish: refuse)
                XCTAssertNil(outcome)
                try fixture.assertRolledBack()
                try source.assertSources()
            }
        }
    }
}
