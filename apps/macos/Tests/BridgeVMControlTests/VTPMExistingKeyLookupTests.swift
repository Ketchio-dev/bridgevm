import Foundation
import LocalAuthentication
import Security
import XCTest
@testable import BridgeVMControl

final class VTPMExistingKeyLookupTests: XCTestCase {
    func testLegacyLookupKeepsEstablishedScopeAndNeverWrites() throws {
        let fixture = VTPMKeychainFixture()
        XCTAssertEqual(try fixture.store.existingStateKey(for: "  stable-vm\n"), fixture.read.data)
        XCTAssertEqual(fixture.calls, ["entitlements", "get", "set:false", "copy", "set:true"])
        let query = try XCTUnwrap(fixture.queries.last)
        XCTAssertEqual(query[kSecAttrAccount as String] as? String, "stable-vm")
        XCTAssertEqual(query[kSecAttrService as String] as? String, "com.bridgevm.vtpm-state-key.v1")
        XCTAssertEqual(query[kSecClass as String] as? String, kSecClassGenericPassword as String)
        XCTAssertEqual(query[kSecMatchLimit as String] as? String, kSecMatchLimitOne as String)
        XCTAssertEqual(query[kSecReturnData as String] as? Bool, true)
        XCTAssertNil(query[kSecUseDataProtectionKeychain as String])
        XCTAssertNil(query[kSecAttrAccessGroup as String]); XCTAssertNil(query[kSecMatchSearchList as String])
        XCTAssertTrue(fixture.writes.isEmpty)
    }

    func testEntitledLookupUsesFreshNoInteractionContextAndNoLegacyPolicy() throws {
        let fixture = VTPMKeychainFixture(); fixture.entitlements = ["keychain-access-groups": ["TEAM.app"]]
        _ = try fixture.store.existingStateKey(for: "vm")
        _ = try fixture.store.existingStateKey(for: "vm")
        XCTAssertEqual(fixture.calls, ["entitlements", "copy", "entitlements", "copy"])
        let first = try XCTUnwrap(fixture.queries[0][kSecUseAuthenticationContext as String] as? LAContext)
        let second = try XCTUnwrap(fixture.queries[1][kSecUseAuthenticationContext as String] as? LAContext)
        XCTAssertEqual(fixture.queryInteractionFlags, [true, true]); XCTAssertFalse(first === second)
        XCTAssertEqual(fixture.queries[0][kSecUseDataProtectionKeychain as String] as? Bool, true)
        XCTAssertNil(fixture.queries[0][kSecUseAuthenticationUI as String])
        XCTAssertTrue(fixture.writes.isEmpty)
    }

    func testAllEntitlementShapesAreValidatedBeforeQuery() throws {
        XCTAssertEqual(try VTPMKeychainBackend.candidate(from: [:]), .legacySearchList)
        XCTAssertEqual(try VTPMKeychainBackend.candidate(from: ["keychain-access-groups": [String]()]), .legacySearchList)
        for name in ["com.apple.application-identifier", "application-identifier"] {
            XCTAssertEqual(try VTPMKeychainBackend.candidate(from: [name: "TEAM.app"]), .dataProtection)
        }
        XCTAssertEqual(try VTPMKeychainBackend.candidate(from: ["com.apple.security.application-groups": ["TEAM.group"]]), .dataProtection)
        for value in [false, "TEAM.app", [""], [1]] as [Any] {
            let fixture = VTPMKeychainFixture(); fixture.entitlements = ["keychain-access-groups": value]
            XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
                XCTAssertEqual($0 as? VTPMExistingKeyError, .malformedEntitlement("keychain-access-groups"))
            }
            XCTAssertEqual(fixture.calls, ["entitlements"])
        }
        let fixture = VTPMKeychainFixture()
        fixture.entitlements = ["keychain-access-groups": ["TEAM.app"], "application-identifier": 42]
        XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm"))
        XCTAssertEqual(fixture.calls, ["entitlements"])
        fixture.entitlementError = VTPMExistingKeyError.backendInspectionFailed
        XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
            XCTAssertEqual($0 as? VTPMExistingKeyError, .backendInspectionFailed)
        }
    }

    func testAbsenceAndAccessErrorsNeverFallbackOrCreate() {
        for entitled in [false, true] {
            for status in [errSecItemNotFound, errSecInteractionNotAllowed, errSecAuthFailed, errSecMissingEntitlement, errSecNotAvailable, errSecDecode] {
                let fixture = VTPMKeychainFixture(); fixture.read = (nil, status)
                if entitled { fixture.entitlements = ["application-identifier": "TEAM.app"] }
                XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) { error in
                    if status == errSecItemNotFound {
                        XCTAssertEqual(error as? VTPMExistingKeyError, .missingKey(entitled ? .dataProtection : .legacySearchList))
                    } else if status == errSecInteractionNotAllowed {
                        XCTAssertEqual(error as? VTPMExistingKeyError, .interactionRequired(status))
                    } else { XCTAssertEqual(error as? VTPMStateSecurityError, .keychainRead(status)) }
                }
                XCTAssertEqual(fixture.calls.filter { $0 == "copy" }.count, 1)
                XCTAssertTrue(fixture.writes.isEmpty)
            }
        }
    }

    func testInvalidIdentifiersAndInvalidResultFailClosed() {
        for id in ["", " \n", String(repeating: "x", count: 256), "vm\u{0000}id"] {
            let fixture = VTPMKeychainFixture()
            XCTAssertThrowsError(try fixture.store.existingStateKey(for: id))
            XCTAssertTrue(fixture.calls.isEmpty)
        }
        for length in [0, 31, 33] {
            let fixture = VTPMKeychainFixture(); fixture.read = (Data(count: length), errSecSuccess)
            XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
                XCTAssertEqual($0 as? VTPMStateSecurityError, .invalidKeyLength(length))
            }
            XCTAssertTrue(fixture.allowed); XCTAssertTrue(fixture.writes.isEmpty)
        }
        let fixture = VTPMKeychainFixture(); fixture.read = (nil, errSecSuccess)
        XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
            XCTAssertEqual($0 as? VTPMStateSecurityError, .keychainRead(errSecDecode))
        }
    }

    func testResolvedInteractiveBackendIsReusedWithoutProbeOrEntitlementRead() throws {
        for legacy in [false, true] {
            let fixture = VTPMKeychainFixture()
            if legacy { fixture.addStatuses = [errSecMissingEntitlement] }
            _ = try fixture.store.stateKey(for: "vm", allowCreation: false)
            fixture.clearHistory(); fixture.entitlementError = VTPMExistingKeyError.backendInspectionFailed
            _ = try fixture.store.existingStateKey(for: "vm")
            XCTAssertFalse(fixture.calls.contains("entitlements")); XCTAssertTrue(fixture.writes.isEmpty)
            XCTAssertEqual(fixture.queries.last?[kSecUseDataProtectionKeychain as String] as? Bool, legacy ? nil : true)
        }
    }

    func testReadOnlyCandidateDoesNotReplaceInteractiveWriteProbe() throws {
        let fixture = VTPMKeychainFixture(); fixture.entitlements = ["application-identifier": "TEAM.app"]
        _ = try fixture.store.existingStateKey(for: "vm")
        fixture.clearHistory(); fixture.addStatuses = [errSecMissingEntitlement]
        _ = try fixture.store.stateKey(for: "vm", allowCreation: false)
        XCTAssertEqual(fixture.calls, ["add", "copy"])
        XCTAssertNil(fixture.queries.last?[kSecUseDataProtectionKeychain as String])
    }
}
