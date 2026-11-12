import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlDisplayCloseTests: XCTestCase {
    func testStoppedAndQuittingCloseWithoutAQuestion() {
        XCTAssertEqual(ControlDisplayClosePlan.make(running: false, quitting: false, canPause: false, remembered: nil), .close)
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: true, canPause: true, remembered: .shutdown), .close)
    }
    func testRunningAsksOnceOrUsesRememberedChoiceWhenAvailable() {
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: true, remembered: nil),
                       .ask([.background, .pause, .shutdown]))
        for choice in ControlDisplayCloseChoice.allCases {
            XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: true, remembered: choice), .apply(choice))
        }
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: false, remembered: .pause),
                       .ask([.background, .shutdown]))
    }
    func testAlertHasOnlyAvailableActionsAndRememberCheckbox() {
        let alert = ControlDisplayClose.alert(choices: [.background, .shutdown])
        XCTAssertEqual(alert.buttons.map(\.title), ["Windows를 백그라운드에서 계속 실행", "Windows 종료"])
        XCTAssertEqual(alert.suppressionButton?.title, "다시 묻지 않음")
    }
    func testRenderCloseAlertOffscreen() throws {
        guard let path = ProcessInfo.processInfo.environment["BV_START_CLOSE_RENDER"] else { throw XCTSkip("Opt-in offscreen render") }
        let alert = ControlDisplayClose.alert(choices: [.background, .pause, .shutdown])
        alert.layout()
        guard let view = alert.window.contentView,
              let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return XCTFail("No alert image") }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        let data = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        try data.write(to: URL(fileURLWithPath: path))
    }
}
