import Foundation
import Security

protocol VTPMExistingStateKeyReading {
    /// Never creates a key, probes by writing, prompts, or retries another backend.
    func existingStateKey(for stableVMID: String) throws -> Data
}

enum VTPMKeychainBackend: String, Equatable {
    case dataProtection, legacySearchList

    static let entitlementNames = ["keychain-access-groups", "com.apple.application-identifier",
                                   "application-identifier", "com.apple.security.application-groups"]

    static func candidate(from values: [String: Any]) throws -> Self {
        var entitled = false
        for name in entitlementNames {
            guard let value = values[name] else { continue }
            if name == "keychain-access-groups" || name == "com.apple.security.application-groups" {
                guard let groups = value as? [String], groups.allSatisfy({ !$0.isEmpty }) else {
                    throw VTPMExistingKeyError.malformedEntitlement(name)
                }
                entitled = entitled || !groups.isEmpty
            } else {
                guard let identifier = value as? String, !identifier.isEmpty else {
                    throw VTPMExistingKeyError.malformedEntitlement(name)
                }
                entitled = true
            }
        }
        return entitled ? .dataProtection : .legacySearchList
    }
}

enum VTPMExistingKeyError: LocalizedError, Equatable {
    case busy, serviceUnavailable, backendInspectionFailed
    case malformedEntitlement(String)
    case missingKey(VTPMKeychainBackend)
    case interactionRequired(OSStatus)
    case interactionPolicy(operation: String, status: OSStatus)
    case interactionRestore(primary: OSStatus, restore: OSStatus)

    var errorDescription: String? {
        switch self {
        case .busy: return "Another Keychain operation is in progress. Try again in a moment."
        case .serviceUnavailable: return "BridgeVM stopped key operations because it couldn’t confirm that Keychain interaction settings were restored."
        case .backendInspectionFailed: return "BridgeVM couldn’t read its Keychain entitlements."
        case let .malformedEntitlement(name): return "The app has an invalid Keychain entitlement (\(name))."
        case let .missingKey(backend): return "The selected Keychain backend (\(backend.rawValue)) has no existing vTPM key. BridgeVM checked only that backend and left it unchanged."
        case let .interactionRequired(status): return "Keychain needs your confirmation to access the vTPM key. Open BridgeVM to confirm (OSStatus \(status))."
        case let .interactionPolicy(operation, status): return "Couldn’t set the Keychain interaction policy (\(operation), OSStatus \(status))."
        case let .interactionRestore(primary, restore): return "Couldn’t restore the Keychain interaction policy (lookup OSStatus \(primary), restore OSStatus \(restore))."
        }
    }
}

enum VTPMExistingKeyLookup {
    static func account(_ stableVMID: String) throws -> String {
        let account = try lengthCheckedAccount(stableVMID)
        guard !account.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) }) else {
            throw VTPMStateSecurityError.invalidVMIdentifier
        }
        return account
    }

    static func lengthCheckedAccount(_ stableVMID: String) throws -> String {
        let trimmed = stableVMID.trimmingCharacters(in: .whitespacesAndNewlines)
        let maximumBytes = 255
        // Preserve the existing byte spelling when it fits. A directory scan can expand a valid
        // Korean ID beyond the limit even though its composed Keychain account still fits.
        let account = trimmed.utf8.count <= maximumBytes ? trimmed : trimmed.precomposedStringWithCanonicalMapping
        guard !account.isEmpty, account.utf8.count <= maximumBytes else {
            throw VTPMStateSecurityError.invalidVMIdentifier
        }
        return account
    }

    /// The account first, then its other Unicode normalization forms. The keychain matches accounts
    /// byte for byte, and a Korean VM ID taken from a directory name can be decomposed (NFD) where
    /// vm.json and a copied library hold it composed (NFC), or the other way round. Swift `==`
    /// treats the forms as equal, so they are told apart by their UTF-8 bytes.
    static func forms(_ account: String) -> [String] {
        var forms = [account]
        for form in [account.precomposedStringWithCanonicalMapping, account.decomposedStringWithCanonicalMapping]
            where !forms.contains(where: { Array($0.utf8) == Array(form.utf8) }) { forms.append(form) }
        return forms
    }

    static func query(account: String, backend: VTPMKeychainBackend) -> [CFString: Any] {
        var query: [CFString: Any] = [kSecClass: kSecClassGenericPassword,
            kSecAttrService: KeychainVTPMStateKeyStore.service, kSecAttrAccount: account]
        if backend == .dataProtection { query[kSecUseDataProtectionKeychain] = true }
        return query
    }

    static func read(account: String, backend: VTPMKeychainBackend, access: VTPMKeychainAccess) throws -> Data {
        var query = query(account: account, backend: backend)
        query[kSecReturnData] = true; query[kSecMatchLimit] = kSecMatchLimitOne
        var result = try access.existingRead(query, backend: backend)
        guard result.status == errSecSuccess else {
            if result.data != nil { result.data!.resetBytes(in: result.data!.indices) }
            if result.status == errSecItemNotFound { throw VTPMExistingKeyError.missingKey(backend) }
            if result.status == errSecInteractionNotAllowed { throw VTPMExistingKeyError.interactionRequired(result.status) }
            throw VTPMStateSecurityError.keychainRead(result.status)
        }
        guard var key = result.data else { throw VTPMStateSecurityError.keychainRead(errSecDecode) }
        result.data = nil
        guard key.count == KeychainVTPMStateKeyStore.keyLength else {
            let count = key.count; key.resetBytes(in: key.indices)
            throw VTPMStateSecurityError.invalidKeyLength(count)
        }
        return key
    }
}
