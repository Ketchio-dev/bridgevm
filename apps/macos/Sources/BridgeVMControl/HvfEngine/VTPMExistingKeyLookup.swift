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
        case .busy: return "다른 Keychain 작업이 진행 중입니다. 잠시 후 다시 시도하세요."
        case .serviceUnavailable: return "Keychain 상호작용 정책 복원이 확인되지 않아 추가 키 작업을 중단했습니다."
        case .backendInspectionFailed: return "앱의 Keychain 저장소 권한을 읽지 못했습니다."
        case let .malformedEntitlement(name): return "앱의 Keychain 권한 형식이 올바르지 않습니다 (\(name))."
        case let .missingKey(backend): return "선택한 Keychain 저장소(\(backend.rawValue))에 기존 vTPM 키가 없습니다. 다른 저장소를 조회하거나 키를 생성하지 않았습니다."
        case let .interactionRequired(status): return "vTPM 키 접근에 사용자 확인이 필요합니다. 앱에서 확인하세요 (OSStatus \(status))."
        case let .interactionPolicy(operation, status): return "Keychain 상호작용 정책을 설정하지 못했습니다 (\(operation), OSStatus \(status))."
        case let .interactionRestore(primary, restore): return "Keychain 상호작용 정책을 복원하지 못했습니다 (조회 OSStatus \(primary), 복원 OSStatus \(restore))."
        }
    }
}

enum VTPMExistingKeyLookup {
    static func account(_ stableVMID: String) throws -> String {
        let account = stableVMID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !account.isEmpty, account.utf8.count <= 255,
              !account.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
        else { throw VTPMStateSecurityError.invalidVMIdentifier }
        return account
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
