import Foundation
import LocalAuthentication
import Security
@testable import BridgeVMControl

/// Synthetic bytes and injected functions only. Never connects to Security services.
final class VTPMKeychainFixture {
    var calls: [String] = []
    var queries: [[String: Any]] = []
    var queryInteractionFlags: [Bool?] = []
    var entitlements: [String: Any] = [:]
    var entitlementError: Error?
    var reads: [VTPMKeychainAPI.Read] = []
    var read: VTPMKeychainAPI.Read = (Data(repeating: 0x5a, count: 32), errSecSuccess)
    var addStatuses: [OSStatus] = []
    var updateStatus: OSStatus = errSecSuccess
    var deleteStatus: OSStatus = errSecSuccess
    var randomStatus: OSStatus = errSecSuccess
    var getStatus: OSStatus = errSecSuccess
    var setStatuses: [OSStatus] = []
    var allowed = true
    var setMutatesOnFailure = false
    var onCopy: (() -> Void)?
    lazy var access = VTPMKeychainAccess(api: api)
    lazy var store = KeychainVTPMStateKeyStore(access: access)

    var api: VTPMKeychainAPI {
        .init(copy: { [unowned self] query in
            calls.append("copy"); queries.append(query as NSDictionary as! [String: Any])
            queryInteractionFlags.append((queries.last?[kSecUseAuthenticationContext as String] as? LAContext)?.interactionNotAllowed)
            onCopy?()
            return reads.isEmpty ? read : reads.removeFirst()
        }, add: { [unowned self] query in
            calls.append("add"); queries.append(query as NSDictionary as! [String: Any])
            return addStatuses.isEmpty ? errSecSuccess : addStatuses.removeFirst()
        }, update: { [unowned self] query, _ in
            calls.append("update"); queries.append(query as NSDictionary as! [String: Any]); return updateStatus
        }, delete: { [unowned self] query in
            calls.append("delete"); queries.append(query as NSDictionary as! [String: Any]); return deleteStatus
        }, random: { [unowned self] data in
            calls.append("random"); data = Data(repeating: 0x6b, count: data.count); return randomStatus
        }, entitlements: { [unowned self] in
            calls.append("entitlements"); if let entitlementError { throw entitlementError }; return entitlements
        }, getInteraction: { [unowned self] in
            calls.append("get"); return (allowed, getStatus)
        }, setInteraction: { [unowned self] value in
            calls.append(value ? "set:true" : "set:false")
            let status = setStatuses.isEmpty ? errSecSuccess : setStatuses.removeFirst()
            if status == errSecSuccess || setMutatesOnFailure { allowed = value }
            return status
        })
    }

    func clearHistory() { calls = []; queries = [] }
    var writes: [String] { calls.filter { ["add", "update", "delete", "random"].contains($0) } }
}
