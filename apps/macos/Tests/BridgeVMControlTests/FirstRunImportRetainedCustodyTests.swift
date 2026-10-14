import XCTest
@testable import BridgeVMControl

private final class RetainedImportKeyStore: VTPMStateKeyManaging {
    var keys: [String: Data] = [:]
    var deletions: [String] = []
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        guard let key = keys[stableVMID] else { throw VTPMStateSecurityError.missingKeyForExistingState }
        return key
    }
    func replaceStateKey(_ key: Data, for stableVMID: String) throws { keys[stableVMID] = key }
    func deleteStateKey(for stableVMID: String) throws { deletions.append(stableVMID); keys[stableVMID] = nil }
}

final class FirstRunImportRetainedCustodyTests: XCTestCase {
    private let stateBytes = Data("synthetic sealed-state fixture".utf8)
    private let fakeKey = Data(repeating: 0x5a, count: 32)

    private func prepare(_ fixture: VMCreationRegistrationCollisionTests.Fixture,
        store: RetainedImportKeyStore) throws -> FirstRunImportPreparation {
        let destination = try FirstRunImportDestination(slug: "imported",
            libraryRoot: fixture.library, fileManager: .default)
        let bundle = destination.root.appendingPathComponent("bundle")
        let state = bundle.appendingPathComponent("metadata/vtpm")
        let source = fixture.root.appendingPathComponent("synthetic-state")
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: false)
        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: true)
        try stateBytes.write(to: source.appendingPathComponent("tpm2-00.permall"))
        store.keys["source"] = fakeKey
        let package = fixture.root.appendingPathComponent("synthetic-recovery.json")
        let code = fixture.root.appendingPathComponent("synthetic-code.txt")
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source", stateDirectory: source, destination: package)
        try Data(exported.recoveryCode.utf8).write(to: code)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: code.path)
        let inputs = FirstRunImport.Inputs(displayName: "Imported", diskPath: "unused", varsPath: "unused",
            vtpmStateDir: source.path, vtpmRecoveryPackagePath: package.path,
            vtpmRecoveryCodePath: code.path, memMiB: 4096, cpuCount: 2)
        let custody = try FirstRunImportVTPMRecovery.materialize(inputs, destinationStableVMID: "imported",
            destination: state, fileManager: .default, keyStore: store)
        let config = VMConfig(id: "imported", name: "Imported", displayName: "Imported", backendKind: "hvf-engine",
            bundlePath: bundle.path, runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "Imported", displayWidth: 1280, displayHeight: 800)
        return FirstRunImportPreparation(config: config, destination: destination, importedVTPMKey: custody)
    }

    private func publish(_ prepared: FirstRunImportPreparation, root: URL) -> FirstRunImportResult {
        FirstRunImportWorker.finishPublication(VMLibrary.createOutcome(prepared.config, rootURL: root), prepared: prepared)
    }

    func testAmbiguousRegistrationKeepsImportedKeyWithRetainedState() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        let store = RetainedImportKeyStore()
        var prepared: FirstRunImportPreparation? = try prepare(fixture, store: store)
        let entry = fixture.library.appendingPathComponent("imported")
        try FileManager.default.createSymbolicLink(atPath: entry.appendingPathComponent("vm.json").path,
            withDestinationPath: "missing-registration")
        let result = publish(try XCTUnwrap(prepared), root: fixture.library)
        guard case .failed = result else { return XCTFail("ambiguous registration must refuse") }
        prepared = nil
        XCTAssertEqual(try Data(contentsOf: entry.appendingPathComponent("bundle/metadata/vtpm/tpm2-00.permall")), stateBytes)
        XCTAssertEqual(store.keys["imported"], fakeKey, "preserved imported state needs its matching key")
        XCTAssertTrue(store.deletions.isEmpty)
        XCTAssertEqual(store.keys["source"], fakeKey)
    }

    func testUnpublishedOwnedPreparationRemovesItsImportedKey() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        let store = RetainedImportKeyStore()
        var prepared: FirstRunImportPreparation? = try prepare(fixture, store: store)
        XCTAssertNotNil(prepared)
        prepared = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.library.appendingPathComponent("imported").path))
        XCTAssertNil(store.keys["imported"])
        XCTAssertEqual(store.deletions, ["imported"])
        XCTAssertEqual(store.keys["source"], fakeKey)
    }
}
