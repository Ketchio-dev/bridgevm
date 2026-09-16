import Foundation

enum VTPMExistingRuntimeKey {
    static func withExistingKey<T>(for config: HvfEngineConfig, provider: VTPMExistingStateKeyReading,
        fileManager: FileManager = .default, body: (Data?) throws -> T) throws -> T {
        guard let stateDir = config.vtpmStateDir else { return try body(nil) }
        guard let keyID = config.vtpmKeyID else { throw VTPMStateSecurityError.invalidVMIdentifier }
        _ = try VTPMStateSecurity.stateDirectoryContainsData(at: stateDir, fileManager: fileManager)
        var key = try provider.existingStateKey(for: keyID)
        defer { key.resetBytes(in: key.indices) }
        guard key.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMStateSecurityError.invalidKeyLength(key.count)
        }
        return try body(key)
    }
}
