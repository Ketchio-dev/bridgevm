import Foundation

enum VTPMRuntimeKey {
    /// Shares key provenance rules with the legacy transport without retaining key material.
    static func withKey<T>(for config: HvfEngineConfig, provider: VTPMStateKeyProviding,
                           fileManager: FileManager = .default, body: (Data?) throws -> T) throws -> T {
        guard let stateDir = config.vtpmStateDir else { return try body(nil) }
        guard let keyID = config.vtpmKeyID else { throw VTPMStateSecurityError.invalidVMIdentifier }
        let existing = try VTPMStateSecurity.stateDirectoryContainsData(at: stateDir, fileManager: fileManager)
        var key = try provider.stateKey(for: keyID, allowCreation: !existing)
        defer { key.resetBytes(in: key.indices) }
        guard key.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMStateSecurityError.invalidKeyLength(key.count)
        }
        return try body(key)
    }
}
