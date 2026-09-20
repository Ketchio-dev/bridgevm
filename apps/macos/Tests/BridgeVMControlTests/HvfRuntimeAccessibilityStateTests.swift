import XCTest
@testable import BridgeVMControl

final class HvfRuntimeAccessibilityStateTests: XCTestCase {
    func testEveryConnectionStateHasAStableNonlocalizedCode() {
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: false, connection: .stopped), "stopped")
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: false, connection: .booting), "booting")
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: false, connection: .connected(host: "private")), "connected")
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: false, connection: .stopping), "stopping")
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: false, connection: .timedOut), "timed-out")
    }

    func testPendingWorkOverridesTheUnderlyingConnectionState() {
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: true, connection: .stopped), "start-pending")
        XCTAssertEqual(HvfRuntimeAccessibilityState.code(pending: true, connection: .connected(host: "private")), "start-pending")
    }
}
