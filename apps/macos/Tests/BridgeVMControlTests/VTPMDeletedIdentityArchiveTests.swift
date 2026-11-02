import XCTest
@testable import BridgeVMControl

/// Retiring a deleted VM's key copies it before removal and never destroys the
/// only copy; every failure leaves the active key where it was.
final class VTPMDeletedIdentityArchiveTests: XCTestCase {
    private let key = Data(repeating: 0x42, count: KeychainVTPMStateKeyStore.keyLength)
    private let now = Date(timeIntervalSince1970: 1_700_000_000)
    private let nonce = UUID(uuidString: "ABCDEF12-3456-7890-ABCD-EF1234567890")!
    private let archivedID = "windows-11.deleted.1700000000-abcdef12"

    private func archive(_ store: VTPMDeletionTestKeyStore) -> VTPMDeletedIdentityArchive {
        VTPMIdentityLifecycle(keyStore: store).archiveDeletedIdentity(stableVMID: "windows-11", now: now, nonce: nonce)
    }

    func testActiveKeyIsCopiedUnderTheResetSuffixThenRemoved() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["windows-11"] = key
        XCTAssertEqual(archive(store), .archived(keyID: archivedID))
        XCTAssertEqual(store.keys, [archivedID: key])
        XCTAssertEqual(store.calls, ["read windows-11", "replace \(archivedID)", "delete windows-11"])
    }

    func testMissingKeyIsANoOp() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["other-vm"] = key
        XCTAssertEqual(archive(store), .noKey)
        XCTAssertEqual(store.keys, ["other-vm": key])
        XCTAssertEqual(store.calls, ["read windows-11"])
    }

    func testUnreadableKeyIsLeftUntouched() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["windows-11"] = key
        store.readError = VTPMStateSecurityError.keychainRead(-25308)
        guard case .failed = archive(store) else { return XCTFail("an unreadable key must report failure") }
        XCTAssertEqual(store.keys, ["windows-11": key])
        XCTAssertEqual(store.calls, ["read windows-11"])
    }

    func testFailedArchiveCopyKeepsTheActiveKey() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["windows-11"] = key
        store.failsReplace = { $0.contains(".deleted.") }
        guard case .failed = archive(store) else { return XCTFail("a failed copy must report failure") }
        XCTAssertEqual(store.keys, ["windows-11": key])
        XCTAssertFalse(store.calls.contains("delete windows-11"))
    }

    func testFailedActiveRemovalRestoresTheActiveKeyBeforeDroppingTheCopy() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["windows-11"] = key
        store.failsDelete = { $0 == "windows-11" }
        guard case .failed = archive(store) else { return XCTFail("a failed removal must report failure") }
        XCTAssertEqual(store.keys, ["windows-11": key])
        XCTAssertEqual(store.calls, ["read windows-11", "replace \(archivedID)", "delete windows-11",
                                     "replace windows-11", "delete \(archivedID)"])
    }

    func testUnconfirmedRestoreKeepsTheArchiveCopy() {
        let store = VTPMDeletionTestKeyStore()
        store.keys["windows-11"] = key
        store.failsDelete = { $0 == "windows-11" }
        store.failsReplace = { $0 == "windows-11" }
        guard case .failed = archive(store) else { return XCTFail("a failed removal must report failure") }
        XCTAssertEqual(store.keys[archivedID], key)
        XCTAssertFalse(store.calls.contains("delete \(archivedID)"))
    }
}
