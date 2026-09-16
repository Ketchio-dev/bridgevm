import Foundation
import Security
import XCTest
@testable import BridgeVMControl

final class VTPMKeychainAccessTests: XCTestCase {
    func testLegacyRestoresBothInitialPolicyStatesOnReadFailure() {
        for allowed in [false, true] {
            let fixture = VTPMKeychainFixture(); fixture.allowed = allowed; fixture.read = (nil, errSecAuthFailed)
            XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
                XCTAssertEqual($0 as? VTPMStateSecurityError, .keychainRead(errSecAuthFailed))
            }
            XCTAssertEqual(fixture.allowed, allowed)
            XCTAssertEqual(fixture.calls, ["entitlements", "get", "set:false", "copy", "set:\(allowed)"])
        }
    }

    func testPolicyReadAndDisableFailuresNeverReadAKey() {
        let unavailable = VTPMKeychainFixture(); unavailable.getStatus = errSecNotAvailable
        XCTAssertThrowsError(try unavailable.store.existingStateKey(for: "vm")) {
            XCTAssertEqual($0 as? VTPMExistingKeyError, .interactionPolicy(operation: "read", status: errSecNotAvailable))
        }
        XCTAssertEqual(unavailable.calls, ["entitlements", "get"])
        let failed = VTPMKeychainFixture(); failed.setStatuses = [errSecAuthFailed, errSecSuccess]
        failed.setMutatesOnFailure = true
        XCTAssertThrowsError(try failed.store.existingStateKey(for: "vm")) {
            XCTAssertEqual($0 as? VTPMExistingKeyError, .interactionPolicy(operation: "disable", status: errSecAuthFailed))
        }
        XCTAssertEqual(failed.calls, ["entitlements", "get", "set:false", "set:true"])
        XCTAssertTrue(failed.allowed)
    }

    func testRestoreFailureDiscardsResultAndDisablesEveryStoreSharingAccess() {
        for primary in [errSecSuccess, errSecAuthFailed] {
            let fixture = VTPMKeychainFixture(); fixture.read.status = primary
            fixture.setStatuses = [errSecSuccess, errSecNotAvailable]
            XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
                XCTAssertEqual($0 as? VTPMExistingKeyError, .interactionRestore(primary: primary, restore: errSecNotAvailable))
            }
            fixture.clearHistory()
            let other = KeychainVTPMStateKeyStore(access: fixture.access)
            assertUnavailable { _ = try other.existingStateKey(for: "other") }
            assertUnavailable { _ = try other.stateKey(for: "other", allowCreation: true) }
            assertUnavailable { try other.replaceStateKey(Data(count: 32), for: "other") }
            assertUnavailable { try other.deleteStateKey(for: "other") }
            XCTAssertTrue(fixture.calls.isEmpty)
        }
    }

    func testDisableAndRestoreFailurePreserveBothStatuses() {
        let fixture = VTPMKeychainFixture(); fixture.setStatuses = [errSecAuthFailed, errSecNotAvailable]
        XCTAssertThrowsError(try fixture.store.existingStateKey(for: "vm")) {
            XCTAssertEqual($0 as? VTPMExistingKeyError, .interactionRestore(primary: errSecAuthFailed, restore: errSecNotAvailable))
        }
        XCTAssertFalse(fixture.calls.contains("copy"))
        assertUnavailable { _ = try fixture.store.existingStateKey(for: "vm") }
    }

    func testAllStoreMethodsRefuseReentryWhileReadOwnsGlobalPolicy() throws {
        let fixture = VTPMKeychainFixture()
        let other = KeychainVTPMStateKeyStore(access: fixture.access)
        fixture.onCopy = {
            XCTAssertFalse(fixture.allowed)
            let before = fixture.calls
            self.assertBusy { _ = try other.existingStateKey(for: "other") }
            self.assertBusy { _ = try other.stateKey(for: "other", allowCreation: true) }
            self.assertBusy { try other.replaceStateKey(Data(count: 32), for: "other") }
            self.assertBusy { try other.deleteStateKey(for: "other") }
            XCTAssertEqual(fixture.calls, before)
        }
        _ = try fixture.store.existingStateKey(for: "vm")
        XCTAssertTrue(fixture.allowed)
    }

    func testBusyCallerDoesNotWaitForOwnedBackgroundRead() throws {
        let fixture = VTPMKeychainFixture()
        let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        let completed = DispatchSemaphore(value: 0)
        fixture.onCopy = { entered.signal(); XCTAssertEqual(release.wait(timeout: .now() + 2), .success) }
        let store = fixture.store
        DispatchQueue.global().async {
            defer { completed.signal() }
            do { _ = try store.existingStateKey(for: "worker") }
            catch { XCTFail("Synthetic worker failed: \(error)") }
        }
        XCTAssertEqual(entered.wait(timeout: .now() + 1), .success)
        let began = ProcessInfo.processInfo.systemUptime
        assertBusy { _ = try store.stateKey(for: "ui", allowCreation: true) }
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - began, 0.5)
        release.signal(); XCTAssertEqual(completed.wait(timeout: .now() + 2), .success)
        XCTAssertTrue(fixture.allowed); XCTAssertTrue(fixture.writes.isEmpty)
    }

    func testInteractiveDuplicateWinnerAndReplacementSemanticsRemain() throws {
        let fixture = VTPMKeychainFixture()
        fixture.reads = [(nil, errSecItemNotFound), (Data(repeating: 7, count: 32), errSecSuccess)]
        fixture.addStatuses = [errSecSuccess, errSecDuplicateItem]
        XCTAssertEqual(try fixture.store.stateKey(for: "vm", allowCreation: true), Data(repeating: 7, count: 32))
        XCTAssertEqual(fixture.calls, ["add", "delete", "copy", "random", "add", "copy"])
        fixture.clearHistory(); fixture.updateStatus = errSecItemNotFound
        try fixture.store.replaceStateKey(Data(count: 32), for: "vm")
        try fixture.store.deleteStateKey(for: "vm")
        XCTAssertEqual(fixture.calls, ["update", "add", "delete"])
    }

    private func assertBusy(_ body: () throws -> Void) {
        XCTAssertThrowsError(try body()) { XCTAssertEqual($0 as? VTPMExistingKeyError, .busy) }
    }
    private func assertUnavailable(_ body: () throws -> Void) {
        XCTAssertThrowsError(try body()) { XCTAssertEqual($0 as? VTPMExistingKeyError, .serviceUnavailable) }
    }
}
