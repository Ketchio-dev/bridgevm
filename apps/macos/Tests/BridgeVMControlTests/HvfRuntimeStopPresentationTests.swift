import XCTest
@testable import BridgeVMControl

final class HvfRuntimeStopPresentationTests: XCTestCase {
    func testOnlyActiveIdleOwnedRuntimeEnablesStop() {
        let enabled = HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: false, ownsRuntime: true)
        XCTAssertTrue(enabled.isEnabled)
        XCTAssertEqual(enabled.guidance, "이 앱이 소유한 VM을 정지합니다.")

        for state in [(false, false, true), (true, true, true), (true, false, false)] {
            XCTAssertFalse(HvfRuntimeStopPresentation.make(
                runtimeActive: state.0, lifecycleBusy: state.1, ownsRuntime: state.2).isEnabled)
        }
    }

    func testUnavailableReasonsDistinguishBusyExternalAndStopped() {
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: true, ownsRuntime: false).guidance,
            "현재 작업이 끝난 뒤 VM을 정지할 수 있습니다.")
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: true, lifecycleBusy: false, ownsRuntime: false).guidance,
            "이 앱이 시작하고 소유한 VM만 안전하게 정지할 수 있습니다.")
        XCTAssertEqual(HvfRuntimeStopPresentation.make(
            runtimeActive: false, lifecycleBusy: false, ownsRuntime: true).guidance,
            "VM이 실행 중이 아닙니다.")
    }
}
