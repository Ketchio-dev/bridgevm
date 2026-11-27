import XCTest
@testable import BridgeVMControl

private final class ImportedRecoveryKeyStore: VTPMStateKeyManaging {
    var keys: [String: Data] = [:]
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        if let key = keys[stableVMID] { return key }
        guard allowCreation else { throw VTPMStateSecurityError.missingKeyForExistingState }
        let key = Data(repeating: 0xa5, count: 32); keys[stableVMID] = key; return key
    }
    func replaceStateKey(_ key: Data, for stableVMID: String) throws { keys[stableVMID] = key }
    func deleteStateKey(for stableVMID: String) throws { keys[stableVMID] = nil }
}

final class VTPMImportedRecoveryTests: XCTestCase {
    func testExplicitRecoveryMigratesKeyAndCustodyRollsBack() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        let source = root.appendingPathComponent("source"), destination = root.appendingPathComponent("destination")
        let package = root.appendingPathComponent("recovery.json"), code = root.appendingPathComponent("code.txt")
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("sealed-state".utf8).write(to: source.appendingPathComponent("tpm2-00.permall"))
        try Data().write(to: source.appendingPathComponent(".lock"))
        let store = ImportedRecoveryKeyStore(); let original = Data(repeating: 0x42, count: 32)
        store.keys["source-vm"] = original
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source-vm", stateDirectory: source, destination: package)
        try Data(exported.recoveryCode.utf8).write(to: code); try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: code.path)
        let inputs = FirstRunImport.Inputs(displayName: "Imported", diskPath: "disk", varsPath: "vars",
            vtpmStateDir: source.path, vtpmRecoveryPackagePath: package.path,
            vtpmRecoveryCodePath: code.path, memMiB: 4096, cpuCount: 4)
        var custody: ImportedVTPMKeyCustody? = try FirstRunImportVTPMRecovery.materialize(inputs,
            destinationStableVMID: "imported-vm", destination: destination,
            fileManager: .default, keyStore: store)
        XCTAssertEqual(store.keys["imported-vm"], original)
        XCTAssertTrue(FileManager.default.fileExists(atPath: destination.appendingPathComponent("tpm2-00.permall").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.appendingPathComponent(".lock").path))
        custody = nil; XCTAssertNil(custody); XCTAssertNil(store.keys["imported-vm"])
    }

    /// The app shows the code on screen, so a code file the user saved is usually readable by
    /// others, and a code can be mistyped: both used to fail after the whole disk copy, with the
    /// sentence about disk access and free space.
    func testRecoveryProblemsAreNamedBeforeTheDiskIsCopied() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let state = fixture.root.appendingPathComponent("state"), package = fixture.root.appendingPathComponent("recovery.json")
        let code = fixture.root.appendingPathComponent("code.txt")
        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: true)
        try Data("sealed-state".utf8).write(to: state.appendingPathComponent("tpm2-00.permall"))
        let store = ImportedRecoveryKeyStore(); store.keys["source-vm"] = Data(repeating: 0x42, count: 32)
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source-vm", stateDirectory: state, destination: package)
        try Data(exported.recoveryCode.utf8).write(to: code)
        try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: code.path)
        var inputs = fixture.inputs()
        inputs.vtpmStateDir = state.path; inputs.vtpmRecoveryPackagePath = package.path; inputs.vtpmRecoveryCodePath = code.path
        func failure() -> String? {
            let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
                snapshotHelper: HvfMediaImportTestSupport.helper)
            guard case .failed(let message) = result else { return nil }
            return message
        }
        XCTAssertEqual(failure(), FirstRunImport.ValidationError.vtpmRecoveryCodeShared(code.path).description)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path), "refused before any copy")
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: code.path)
        let wrong = String(exported.recoveryCode.reversed())
        try Data(wrong.utf8).write(to: code)
        let message = failure()
        XCTAssertTrue(message == VTPMIdentityLifecycleError.invalidRecoveryPackage.localizedDescription
            || message == VTPMIdentityLifecycleError.invalidRecoveryCode.localizedDescription, message ?? "nil")
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        try Data(exported.recoveryCode.utf8).write(to: code)
        XCTAssertNoThrow(try FirstRunImportVTPMRecovery.check(inputs, keyStore: store))
    }

    func testExistingDestinationKeyIsNeverOverwritten() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        let state = root.appendingPathComponent("state"), package = root.appendingPathComponent("recovery.json")
        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("state".utf8).write(to: state.appendingPathComponent("state.bin"))
        let store = ImportedRecoveryKeyStore(); store.keys["source"] = Data(repeating: 1, count: 32)
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source", stateDirectory: state, destination: package)
        let destination = Data(repeating: 9, count: 32); store.keys["destination"] = destination
        XCTAssertThrowsError(try VTPMIdentityLifecycle(keyStore: store).restoreImportedRecovery(
            destinationStableVMID: "destination", stateDirectory: state,
            packageURL: package, recoveryCode: exported.recoveryCode))
        XCTAssertEqual(store.keys["destination"], destination)
    }
}
