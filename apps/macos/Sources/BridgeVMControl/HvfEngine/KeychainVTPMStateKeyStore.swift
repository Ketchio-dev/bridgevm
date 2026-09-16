import Foundation
import Security

/// Existing account and store scope are shared by GUI custody and noninteractive launch.
/// Only data-protection items implement the ThisDeviceOnly accessibility class.
final class KeychainVTPMStateKeyStore: VTPMStateKeyManaging, VTPMExistingStateKeyReading, @unchecked Sendable {
    static let service = "com.bridgevm.vtpm-state-key.v1"
    static let keyLength = 32
    private let access: VTPMKeychainAccess
    // Accessed only under the shared operation gate. A read-only candidate is not a write probe result.
    private var resolvedBackend: VTPMKeychainBackend?

    init(access: VTPMKeychainAccess = .shared) { self.access = access }

    func existingStateKey(for stableVMID: String) throws -> Data {
        let account = try VTPMExistingKeyLookup.account(stableVMID)
        return try access.exclusively {
            let backend = try resolvedBackend ?? VTPMKeychainBackend.candidate(from: access.api.entitlements())
            return try VTPMExistingKeyLookup.read(account: account, backend: backend, access: access)
        }
    }

    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        let account = try VTPMExistingKeyLookup.account(stableVMID)
        return try access.exclusively { try interactiveKey(account: account, allowCreation: allowCreation) }
    }

    private func interactiveKey(account: String, allowCreation: Bool) throws -> Data {
        let existing = read(account: account)
        if existing.status == errSecSuccess, let key = existing.data {
            guard key.count == Self.keyLength else { throw VTPMStateSecurityError.invalidKeyLength(key.count) }
            return key
        }
        guard existing.status == errSecItemNotFound else { throw VTPMStateSecurityError.keychainRead(existing.status) }
        guard allowCreation else { throw VTPMStateSecurityError.missingKeyForExistingState }
        var generated = Data(count: Self.keyLength)
        let randomStatus = access.api.random(&generated)
        guard randomStatus == errSecSuccess else {
            generated.resetBytes(in: generated.indices)
            throw VTPMStateSecurityError.randomGeneration(randomStatus)
        }
        let addStatus = access.api.add(addAttributes(account: account, key: generated))
        if addStatus == errSecSuccess { return generated }
        generated.resetBytes(in: generated.indices)
        if addStatus == errSecDuplicateItem {
            let winner = read(account: account)
            guard winner.status == errSecSuccess, let key = winner.data else {
                throw VTPMStateSecurityError.keychainRead(winner.status)
            }
            guard key.count == Self.keyLength else { throw VTPMStateSecurityError.invalidKeyLength(key.count) }
            return key
        }
        throw VTPMStateSecurityError.keychainWrite(addStatus)
    }

    func replaceStateKey(_ key: Data, for stableVMID: String) throws {
        guard key.count == Self.keyLength else { throw VTPMStateSecurityError.invalidKeyLength(key.count) }
        let account = try VTPMExistingKeyLookup.account(stableVMID)
        try access.exclusively {
            let updateStatus = access.api.update(queryAttributes(account: account) as CFDictionary, [kSecValueData: key] as CFDictionary)
            if updateStatus == errSecSuccess { return }
            guard updateStatus == errSecItemNotFound else { throw VTPMStateSecurityError.keychainWrite(updateStatus) }
            let addStatus = access.api.add(addAttributes(account: account, key: key))
            guard addStatus == errSecSuccess else { throw VTPMStateSecurityError.keychainWrite(addStatus) }
        }
    }

    func deleteStateKey(for stableVMID: String) throws {
        let account = try VTPMExistingKeyLookup.account(stableVMID)
        try access.exclusively {
            let status = access.api.delete(queryAttributes(account: account) as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else { throw VTPMStateSecurityError.keychainWrite(status) }
        }
    }

    private func queryAttributes(account: String) -> [CFString: Any] {
        VTPMExistingKeyLookup.query(account: account, backend: interactiveBackend())
    }

    private func addAttributes(account: String, key: Data) -> CFDictionary {
        var attributes = queryAttributes(account: account)
        attributes[kSecValueData] = key
        attributes[kSecAttrAccessible] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        return attributes as CFDictionary
    }

    /// Preserve the existing interactive probe/fallback, now serialized with every key operation.
    /// Existing-only lookup never invokes this method or caches a speculative write capability.
    private func interactiveBackend() -> VTPMKeychainBackend {
        if let resolvedBackend { return resolvedBackend }
        let account = "bridgevm-entitlement-probe"
        var attributes = VTPMExistingKeyLookup.query(account: account, backend: .dataProtection)
        attributes[kSecValueData] = Data(count: 1)
        attributes[kSecAttrAccessible] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = access.api.add(attributes as CFDictionary)
        let backend: VTPMKeychainBackend
        if status == errSecSuccess || status == errSecDuplicateItem {
            _ = access.api.delete(VTPMExistingKeyLookup.query(account: account, backend: .dataProtection) as CFDictionary)
            backend = .dataProtection
        } else { backend = .legacySearchList }
        resolvedBackend = backend
        return backend
    }

    private func read(account: String) -> VTPMKeychainAPI.Read {
        var query = queryAttributes(account: account)
        query[kSecReturnData] = true; query[kSecMatchLimit] = kSecMatchLimitOne
        let result = access.api.copy(query as CFDictionary)
        if result.status == errSecSuccess && result.data == nil { return (nil, errSecDecode) }
        return result
    }
}
