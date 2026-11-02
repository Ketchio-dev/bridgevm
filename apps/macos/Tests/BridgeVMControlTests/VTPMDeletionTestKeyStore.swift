import Foundation
@testable import BridgeVMControl

/// In-memory vTPM key store for the deletion tests. Synthetic bytes only; it
/// never connects to the Keychain. Created keys differ from every seeded key.
final class VTPMDeletionTestKeyStore: VTPMStateKeyManaging, @unchecked Sendable {
    var keys: [String: Data] = [:]
    private(set) var calls: [String] = []
    var readError: Error?
    var failsReplace: (String) -> Bool = { _ in false }
    var failsDelete: (String) -> Bool = { _ in false }
    private var created: UInt8 = 0

    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        calls.append(allowCreation ? "create \(stableVMID)" : "read \(stableVMID)")
        if let readError { throw readError }
        if let key = keys[stableVMID] { return key }
        guard allowCreation else { throw VTPMStateSecurityError.missingKeyForExistingState }
        created += 1
        let key = Data(repeating: 0xc0 &+ created, count: KeychainVTPMStateKeyStore.keyLength)
        keys[stableVMID] = key
        return key
    }

    func replaceStateKey(_ key: Data, for stableVMID: String) throws {
        calls.append("replace \(stableVMID)")
        if failsReplace(stableVMID) { throw VTPMStateSecurityError.keychainWrite(-61) }
        keys[stableVMID] = key
    }

    func deleteStateKey(for stableVMID: String) throws {
        calls.append("delete \(stableVMID)")
        if failsDelete(stableVMID) { throw VTPMStateSecurityError.keychainWrite(-61) }
        keys[stableVMID] = nil
    }

    func archivedKeyIDs(of stableVMID: String) -> [String] {
        keys.keys.filter { $0.hasPrefix(stableVMID + ".deleted.") }.sorted()
    }
}

extension HvfPendingDeletionFixture {
    /// The fixture's managed VM after installation: deleting it takes the
    /// ordinary media lease, served by the fixture's scripted helper.
    func installedManaged() throws -> VMConfig {
        var installed = config
        installed.installPending = false
        try save(installed)
        try file("disks/hvf-target.raw")
        try file("metadata/hvf-vars.fd")
        return installed
    }

    /// The real library deletion of `selected`, with the scripted lease helper.
    func scriptedDelete(_ selected: VMConfig, _ rootURL: URL) -> Bool {
        rootURL == library && (try? delete(selected)) == true
    }
}
