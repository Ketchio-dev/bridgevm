import XCTest
@testable import BridgeVMControl

/// The picker's help must not claim the window size is ignored once the guest follows it (A8).
final class HvfGuestResolutionPickerHelpTests: XCTestCase {
    func testAVirtioDisplayVMSaysTheDisplayFollowsTheWindow() {
        let text = HvfGuestResolutionPicker.helpText(followsWindow: true, persists: false)
        XCTAssertTrue(text.contains("창 크기를 따라갑니다"), text)
        XCTAssertFalse(text.contains("바뀌지 않습니다"), text)
        XCTAssertFalse(text.contains("드라이버를 설치하세요"), text)
    }

    func testARamfbVMPointsToTheDriverInstall() {
        let text = HvfGuestResolutionPicker.helpText(followsWindow: false, persists: false)
        XCTAssertTrue(text.contains("디스플레이 드라이버를 설치하세요"), text)
        XCTAssertFalse(text.contains("창 크기를 따라갑니다"), text)
    }

    func testTheLibraryNoteIsAppendedOnlyWhenSaved() {
        for follows in [false, true] {
            XCTAssertTrue(HvfGuestResolutionPicker.helpText(followsWindow: follows, persists: true)
                .hasSuffix(" VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다."))
            XCTAssertFalse(HvfGuestResolutionPicker.helpText(followsWindow: follows, persists: false)
                .contains("저장됩니다"))
        }
    }
}
