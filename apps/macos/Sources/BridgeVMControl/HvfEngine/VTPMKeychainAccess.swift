import Foundation
import LocalAuthentication
import Security

struct VTPMKeychainAPI {
    typealias Read = (data: Data?, status: OSStatus)
    var copy: (CFDictionary) -> Read
    var add: (CFDictionary) -> OSStatus
    var update: (CFDictionary, CFDictionary) -> OSStatus
    var delete: (CFDictionary) -> OSStatus
    var random: (inout Data) -> OSStatus
    var entitlements: () throws -> [String: Any]
    var getInteraction: () -> (allowed: Bool, status: OSStatus)
    var setInteraction: (Bool) -> OSStatus

    static let live = Self(
        copy: { query in
            var item: CFTypeRef?
            let status = SecItemCopyMatching(query, &item)
            guard status == errSecSuccess else { return (nil, status) }
            guard let data = item as? Data else { return (nil, errSecDecode) }
            return (data, status)
        },
        add: { SecItemAdd($0, nil) }, update: { SecItemUpdate($0, $1) }, delete: { SecItemDelete($0) },
        random: { data in data.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, $0.count, $0.baseAddress!) } },
        entitlements: {
            guard let task = SecTaskCreateFromSelf(nil) else { throw VTPMExistingKeyError.backendInspectionFailed }
            var error: Unmanaged<CFError>?
            let values = SecTaskCopyValuesForEntitlements(task, VTPMKeychainBackend.entitlementNames as CFArray, &error)
            if let error { _ = error.takeRetainedValue(); throw VTPMExistingKeyError.backendInspectionFailed }
            guard let result = values as? [String: Any] else { throw VTPMExistingKeyError.backendInspectionFailed }
            return result
        },
        getInteraction: { var allowed = DarwinBoolean(false); let status = SecKeychainGetUserInteractionAllowed(&allowed); return (allowed.boolValue, status) },
        setInteraction: { SecKeychainSetUserInteractionAllowed($0) })
}

/// All BridgeVM Keychain operations share this gate. Busy callers never wait on MainActor.
/// Security calls remain synchronous and owned until return; this is not a cancellation API.
final class VTPMKeychainAccess: @unchecked Sendable {
    static let shared = VTPMKeychainAccess(api: .live)
    let api: VTPMKeychainAPI
    private let lock = NSLock()
    private var unavailable = false

    init(api: VTPMKeychainAPI) { self.api = api }

    func exclusively<T>(_ body: () throws -> T) throws -> T {
        guard lock.try() else { throw VTPMExistingKeyError.busy }
        defer { lock.unlock() }
        guard !unavailable else { throw VTPMExistingKeyError.serviceUnavailable }
        return try body()
    }

    /// Called only while the gate is held. Only the Security read runs under altered policy.
    func existingRead(_ query: [CFString: Any], backend: VTPMKeychainBackend) throws -> VTPMKeychainAPI.Read {
        if backend == .dataProtection {
            let context = LAContext(); context.interactionNotAllowed = true
            defer { context.invalidate() }
            var query = query; query[kSecUseAuthenticationContext] = context
            return api.copy(query as CFDictionary)
        }
        let previous = api.getInteraction()
        guard previous.status == errSecSuccess else {
            throw VTPMExistingKeyError.interactionPolicy(operation: "read", status: previous.status)
        }
        let disabled = api.setInteraction(false)
        var result: VTPMKeychainAPI.Read = (nil, disabled)
        if disabled == errSecSuccess { result = api.copy(query as CFDictionary) }
        let restored = api.setInteraction(previous.allowed)
        guard restored == errSecSuccess else {
            unavailable = true
            if result.data != nil { result.data!.resetBytes(in: result.data!.indices); result.data = nil }
            throw VTPMExistingKeyError.interactionRestore(primary: result.status, restore: restored)
        }
        guard disabled == errSecSuccess else {
            throw VTPMExistingKeyError.interactionPolicy(operation: "disable", status: disabled)
        }
        return result
    }
}
