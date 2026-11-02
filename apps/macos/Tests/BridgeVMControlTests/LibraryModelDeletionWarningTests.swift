import Foundation
import XCTest
@testable import BridgeVMControl

/// The Library's delete action reports a completed deletion whose key could not
/// be archived as done plus a warning, never as a deletion failure.
final class LibraryModelDeletionWarningTests: XCTestCase {
    @MainActor
    private func deleteThroughLibrary(_ f: HvfPendingDeletionFixture,
                                      store: VTPMDeletionTestKeyStore) async throws -> LibraryModel {
        let jobs = LibraryDeletionWarningJobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) },
            deletionOperation: { config, root, isRunning in
                LibraryDeletionKeyArchive.perform(config, rootURL: root, isRunning: isRunning,
                                                  keyStore: store, delete: f.scriptedDelete)
            }, startsModelsAutomatically: false)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == f.config.slug })
        library.requestDeletion(listed)
        XCTAssertEqual(library.pendingDeletion, listed)
        library.confirmDeletion(listed)
        let job = try XCTUnwrap(jobs.pending.first)
        await job()
        XCTAssertNil(library.deletionError)
        XCTAssertFalse(library.vms.contains { $0.slug == listed.slug })
        XCTAssertFalse(library.deletingSlugs.contains(listed.slug))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        return library
    }

    @MainActor
    func testUnarchivedKeyIsAWarningOnACompletedDeletion() async throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try f.installedManaged()
        let key = Data(repeating: 0x33, count: KeychainVTPMStateKeyStore.keyLength)
        let store = VTPMDeletionTestKeyStore()
        store.keys[installed.slug] = key
        store.failsReplace = { $0.contains(".deleted.") }
        let library = try await deleteThroughLibrary(f, store: store)
        let warning = try XCTUnwrap(library.deletionWarning)
        XCTAssertTrue(warning.contains("'\(installed.slug)'"), warning)
        XCTAssertEqual(store.keys, [installed.slug: key])
    }

    @MainActor
    func testArchivedKeyLeavesNoWarning() async throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try f.installedManaged()
        let store = VTPMDeletionTestKeyStore()
        store.keys[installed.slug] = Data(repeating: 0x34, count: KeychainVTPMStateKeyStore.keyLength)
        let library = try await deleteThroughLibrary(f, store: store)
        XCTAssertNil(library.deletionWarning)
        XCTAssertNil(store.keys[installed.slug])
        XCTAssertEqual(store.archivedKeyIDs(of: installed.slug).count, 1)
    }
}

@MainActor
private final class LibraryDeletionWarningJobs {
    var pending: [LibraryActionJob] = []
}
