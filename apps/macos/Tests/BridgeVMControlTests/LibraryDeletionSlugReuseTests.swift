import Foundation
import XCTest
@testable import BridgeVMControl

/// After a managed HVF deletion its slug is free again: a new VM with that slug
/// gets its own key, and a first-run vTPM recovery import into it is accepted.
final class LibraryDeletionSlugReuseTests: XCTestCase {
    private let deletedKey = Data(repeating: 0x21, count: KeychainVTPMStateKeyStore.keyLength)

    private func deleteManaged(_ f: HvfPendingDeletionFixture, store: VTPMDeletionTestKeyStore) throws -> VMConfig {
        let installed = try f.installedManaged()
        store.keys[installed.slug] = deletedKey
        let result = LibraryDeletionKeyArchive.perform(installed, rootURL: f.library, isRunning: { false },
                                                       keyStore: store, delete: f.scriptedDelete)
        XCTAssertEqual(result, LibraryDeletionResult(outcome: .deleted))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        return installed
    }

    func testSameSlugVMAfterDeletionStartsWithAFreshKey() throws {
        let f = try HvfPendingDeletionFixture()
        let store = VTPMDeletionTestKeyStore()
        let deleted = try deleteManaged(f, store: store)
        // A recreated VM with the same name has the same slug and managed paths.
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(deleted, rootURL: f.library))
        XCTAssertEqual(launch.vtpmKeyID, deleted.slug)
        var fresh: Data?
        try VTPMRuntimeKey.withKey(for: launch, provider: store) { fresh = $0 }
        XCTAssertNotNil(fresh)
        XCTAssertNotEqual(fresh, deletedKey)
        XCTAssertEqual(store.keys[deleted.slug], fresh)
        XCTAssertEqual(store.calls.last, "create \(deleted.slug)")
        let archived = try XCTUnwrap(store.archivedKeyIDs(of: deleted.slug).first)
        XCTAssertEqual(store.keys[archived], deletedKey)
    }

    func testVTPMRecoveryImportIntoTheDeletedSlugIsNoLongerRefused() throws {
        let f = try HvfPendingDeletionFixture()
        let source = f.root.appendingPathComponent("import-source"), destination = f.root.appendingPathComponent("import-state")
        let package = f.root.appendingPathComponent("recovery.json"), code = f.root.appendingPathComponent("code.txt")
        try f.fm.createDirectory(at: source, withIntermediateDirectories: true)
        try f.fm.createDirectory(at: destination, withIntermediateDirectories: true)
        try Data("sealed-state".utf8).write(to: source.appendingPathComponent("tpm2-00.permall"))
        let store = VTPMDeletionTestKeyStore()
        let sourceKey = Data(repeating: 0x42, count: KeychainVTPMStateKeyStore.keyLength)
        store.keys["source-vm"] = sourceKey
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source-vm", stateDirectory: source, destination: package)
        try Data(exported.recoveryCode.utf8).write(to: code)
        try f.fm.setAttributes([.posixPermissions: 0o600], ofItemAtPath: code.path)
        let slug = f.config.slug
        store.keys[slug] = deletedKey
        // The defect: a leftover key under the slug refuses the import.
        XCTAssertThrowsError(try VTPMIdentityLifecycle(keyStore: store).restoreImportedRecovery(
            destinationStableVMID: slug, stateDirectory: source, packageURL: package,
            recoveryCode: exported.recoveryCode)) {
            XCTAssertEqual($0 as? VTPMImportedRecoveryError, VTPMImportedRecoveryError.destinationIdentityExists(slug))
        }
        XCTAssertEqual(try deleteManaged(f, store: store).slug, slug)
        let inputs = FirstRunImport.Inputs(displayName: "Windows 11", diskPath: "disk", varsPath: "vars",
            vtpmStateDir: source.path, vtpmRecoveryPackagePath: package.path,
            vtpmRecoveryCodePath: code.path, memMiB: 4096, cpuCount: 4)
        let custody = try XCTUnwrap(FirstRunImportVTPMRecovery.materialize(inputs,
            destinationStableVMID: slug, destination: destination, fileManager: .default, keyStore: store))
        custody.preserve()
        XCTAssertEqual(store.keys[slug], sourceKey)
        let archived = try XCTUnwrap(store.archivedKeyIDs(of: slug).first)
        XCTAssertEqual(store.keys[archived], deletedKey)
    }
}
