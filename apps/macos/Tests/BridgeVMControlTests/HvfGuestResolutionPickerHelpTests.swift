import XCTest
@testable import BridgeVMControl

/// The picker's help must not claim the window size is ignored once the guest follows it (A8).
final class HvfGuestResolutionPickerHelpTests: XCTestCase {
    func testAVirtioDisplayVMSaysTheDisplayFollowsTheWindow() {
        let text = HvfGuestResolutionPicker.helpText(followsWindow: true, persists: false)
        XCTAssertTrue(text.contains("Windows resizes to fit the window"), text)
        XCTAssertFalse(text.contains("ignores window size"), text)
        XCTAssertFalse(text.contains("install the display driver"), text)
    }

    func testARamfbVMPointsToTheDriverInstall() {
        let text = HvfGuestResolutionPicker.helpText(followsWindow: false, persists: false)
        XCTAssertTrue(text.contains("install the display driver"), text)
        XCTAssertFalse(text.contains("Windows resizes to fit the window"), text)
    }

    func testAutomaticIsExplainedAsKeepingTheLastStartupResolution() {
        for follows in [false, true] {
            XCTAssertTrue(HvfGuestResolutionPicker.helpText(followsWindow: follows, persists: false)
                .hasSuffix(" Automatic keeps the resolution Windows last started with, 1280x800 for a new VM."))
        }
    }

    func testTheLibraryNoteIsAppendedOnlyWhenSaved() {
        for follows in [false, true] {
            XCTAssertTrue(HvfGuestResolutionPicker.helpText(followsWindow: follows, persists: true)
                .hasSuffix(" Changes made while the VM is stopped are saved for future starts."))
            XCTAssertFalse(HvfGuestResolutionPicker.helpText(followsWindow: follows, persists: false)
                .contains("saved for future starts"))
        }
    }
}
