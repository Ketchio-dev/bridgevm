import XCTest
@testable import BridgeVMControl

private final class CleanupRecoveryStore: VTPMStateKeyManaging {
    var keys: [String: Data] = [:]
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        guard let key = keys[stableVMID] else { throw VTPMStateSecurityError.missingKeyForExistingState }
        return key
    }
    func replaceStateKey(_ key: Data, for stableVMID: String) throws { keys[stableVMID] = key }
    func deleteStateKey(for stableVMID: String) throws { keys[stableVMID] = nil }
}

final class VTPMImportedRecoveryCleanupTests: XCTestCase {
    func testCleanupRequiresMatchingPackageStateAndKey() throws {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        let state = root.appendingPathComponent("state"), package = root.appendingPathComponent("recovery.json")
        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("state".utf8).write(to: state.appendingPathComponent("state.bin"))
        let store = CleanupRecoveryStore(), sourceKey = Data(repeating: 0x31, count: 32)
        store.keys["source"] = sourceKey
        let exported = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
            stableVMID: "source", stateDirectory: state, destination: package)
        store.keys["destination"] = Data(repeating: 0x99, count: 32)
        XCTAssertThrowsError(try VTPMIdentityLifecycle(keyStore: store).removeImportedRecovery(
            destinationStableVMID: "destination", stateDirectory: state,
            packageURL: package, recoveryCode: exported.recoveryCode))
        XCTAssertEqual(store.keys["destination"], Data(repeating: 0x99, count: 32))
        store.keys["destination"] = sourceKey
        try VTPMIdentityLifecycle(keyStore: store).removeImportedRecovery(
            destinationStableVMID: "destination", stateDirectory: state,
            packageURL: package, recoveryCode: exported.recoveryCode)
        XCTAssertNil(store.keys["destination"])
    }
}
