import XCTest
@testable import BridgeVMControl

final class HvfRuntimeStopPresentationTests: XCTestCase {
    func testOnlyActiveIdleOwnedRuntimeEnablesStop() {
        let enabled = HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: false, ownsRuntime: true)
        XCTAssertTrue(enabled.isEnabled)
        XCTAssertEqual(enabled.guidance, "Stop the VM started by this app.")

        for state in [(false, false, true), (true, true, true), (true, false, false)] {
            XCTAssertFalse(HvfRuntimeStopPresentation.make(
                runtimeActive: state.0, lifecycleBusy: state.1, ownsRuntime: state.2).isEnabled)
        }
    }

    func testUnavailableReasonsDistinguishBusyExternalAndStopped() {
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: true, ownsRuntime: false).guidance,
            "Wait for the current operation to finish before stopping the VM.")
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: false, ownsRuntime: false).guidance,
            "Stop the VM from the app or tool that started it.")
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: false, lifecycleBusy: false, ownsRuntime: true).guidance,
            "The VM is stopped.")
    }
}
