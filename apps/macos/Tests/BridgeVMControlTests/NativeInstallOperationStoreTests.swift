import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOperationStoreTests: XCTestCase {
    private let digest = String(repeating: "a", count: 64)

    func testSameAndConcurrentOperationReuseOneReservation() throws {
        let store = NativeInstallOperationStore(), firstID = UUID(), secondID = UUID()
        guard case let .accepted(first) = store.reserve(vmID: "vm", digest: digest,
            operationID: firstID, now: 1) else { return XCTFail("expected admission") }
        guard case let .existing(same) = store.reserve(vmID: "vm", digest: digest,
            operationID: firstID, now: 2) else { return XCTFail("expected idempotent result") }
        guard case let .existing(concurrent) = store.reserve(vmID: "vm", digest: digest,
            operationID: secondID, now: 3) else { return XCTFail("expected existing work") }
        XCTAssertTrue(first === same); XCTAssertTrue(first === concurrent)
        XCTAssertEqual(try first.observation().acceptedUptime, 1)
        XCTAssertTrue(store.isActive(vmID: "vm"))
    }

    func testTerminalFailureAllowsNewExplicitRetry() throws {
        let store = NativeInstallOperationStore(), firstID = UUID(), retryID = UUID()
        guard case let .accepted(first) = store.reserve(vmID: "vm", digest: digest,
            operationID: firstID) else { return XCTFail("expected admission") }
        first.fail("failed")
        guard case let .existing(replayed) = store.reserve(vmID: "vm", digest: digest,
            operationID: firstID) else { return XCTFail("expected terminal replay") }
        guard case let .accepted(retry) = store.reserve(vmID: "vm", digest: digest,
            operationID: retryID) else { return XCTFail("expected retry") }
        XCTAssertTrue(first === replayed); XCTAssertFalse(first === retry)
        XCTAssertTrue(store.isActive(vmID: "vm"))
    }

    func testDigestMismatchCapacityAndTerminalRemovalFailClosed() throws {
        let store = NativeInstallOperationStore(capacity: 1)
        guard case let .accepted(operation) = store.reserve(vmID: "vm", digest: digest,
            operationID: UUID()) else { return XCTFail("expected admission") }
        guard case .refused(.configurationChanged) = store.reserve(vmID: "vm", digest: String(repeating: "b", count: 64),
            operationID: UUID()) else { return XCTFail("expected changed config") }
        guard case .refused(.ledgerFull) = store.reserve(vmID: "other", digest: digest,
            operationID: UUID()) else { return XCTFail("expected capacity refusal") }
        store.removeTerminal(vmID: "vm")
        XCTAssertTrue(store.isActive(vmID: "vm"))
        operation.cancel(); store.removeTerminal(vmID: "vm")
        guard case .failure(.targetUnavailable) = store.operation(vmID: "vm", digest: digest) else {
            return XCTFail("expected removed terminal operation")
        }
    }
}
