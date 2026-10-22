import XCTest
@testable import BridgeVMProductE2E

/// T19's first boot must accept the readiness evidence T17 and the receipt verifier accept: when the guest's one-shot
/// READY is dropped, the host's proactive PONG is the only line, and waiting for READY alone times out a booted guest.
final class A9FirstReadyLineTests: XCTestCase {
    func testReadyLineIsAccepted() {
        XCTAssertEqual(A9FirstReadyLine.first(in: ["boot", "BVAGENT READY nonce=x"]), "BVAGENT READY nonce=x")
    }

    func testProactivePongIsAcceptedWhenReadyWasDropped() {
        XCTAssertEqual(A9FirstReadyLine.first(in: ["boot", "BVAGENT PONG (proactive) t=12"]),
            "BVAGENT PONG (proactive) t=12")
    }

    func testOtherAgentLinesAreNotReadiness() {
        XCTAssertNil(A9FirstReadyLine.first(in: ["BVAGENT PONG", "BVAGENT re-READY", "READY", "x BVAGENT READY"]))
    }
}
