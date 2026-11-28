import XCTest
@testable import BridgeVMControl

final class HvfRuntimeAccessibilityStateTests: XCTestCase {
    private func state(pending: Bool = false, _ connection: HvfConnectionState) -> String {
        HvfRuntimeAccessibilityState.identifier(pending: pending, connection: connection)
    }

    func testEveryConnectionStateHasAStableNonlocalizedIdentifier() {
        XCTAssertEqual(state(.stopped), "bridgevm.windows.runtime.state.stopped")
        XCTAssertEqual(state(.booting), "bridgevm.windows.runtime.state.booting")
        XCTAssertEqual(state(.connected(host: "private")), "bridgevm.windows.runtime.state.connected")
        XCTAssertEqual(state(.stopping), "bridgevm.windows.runtime.state.stopping")
        XCTAssertEqual(state(.timedOut), "bridgevm.windows.runtime.state.timed-out")
    }

    func testPendingWorkOverridesTheUnderlyingConnectionState() {
        XCTAssertEqual(state(pending: true, .stopped), "bridgevm.windows.runtime.state.start-pending")
        XCTAssertEqual(state(pending: true, .connected(host: "private")), "bridgevm.windows.runtime.state.start-pending")
    }

    func testOnlyTheIdentifierSaysThatThereIsNoStartupError() {
        XCTAssertEqual(HvfRuntimeAccessibilityState.failureIdentifier(nil), "bridgevm.windows.runtime.start.failure.none")
        XCTAssertEqual(HvfRuntimeAccessibilityState.failureIdentifier("Windows didn’t start."),
                       "bridgevm.windows.runtime.start.failure")
    }
}
