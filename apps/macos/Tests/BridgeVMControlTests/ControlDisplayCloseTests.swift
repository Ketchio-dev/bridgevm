import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlDisplayCloseTests: XCTestCase {
    func testCloseQuestionIsASheetNotAModalRunLoop() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().appendingPathComponent("Sources/BridgeVMControl/HvfEngine/HvfLiveDisplay.swift")
        let source = try String(contentsOf: root)
        XCTAssertTrue(source.contains("beginSheetModal(for: sender)"), "Close question must keep the default run loop polling")
        XCTAssertFalse(source.contains("alert.runModal()"), "Close must not enter a modal polling dead zone")
    }
    func testStoppedAndQuittingCloseWithoutAQuestion() {
        XCTAssertEqual(ControlDisplayClosePlan.make(running: false, quitting: false, canPause: false, remembered: nil), .close)
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: true, canPause: true, remembered: .shutdown), .close)
    }
    func testRunningAsksOnceOrUsesRememberedChoiceWhenAvailable() {
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: true, remembered: nil),
                       .ask([.background, .pause, .shutdown, .cancel]))
        for choice in [ControlDisplayCloseChoice.background, .pause, .shutdown] {
            XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: true, remembered: choice), .apply(choice))
        }
        XCTAssertEqual(ControlDisplayClosePlan.make(running: true, quitting: false, canPause: false, remembered: .pause),
                       .ask([.background, .shutdown, .cancel]))
    }
    func testAlertHasOnlyAvailableActionsAndRememberCheckbox() {
        let alert = ControlDisplayClose.alert(choices: [.background, .shutdown, .cancel])
        XCTAssertEqual(alert.buttons.map(\.title), ["백그라운드에서 계속 실행", "Windows 종료", "취소"])
        XCTAssertEqual(alert.buttons.last?.keyEquivalent, "\u{1b}")
        XCTAssertEqual(alert.suppressionButton?.title, "다시 묻지 않음")
    }
    func testCancelChangesNothingAndRememberedShutdownCanBeReset() throws {
        let suite = "start-close-" + UUID().uuidString
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        ControlDisplayClose.remember(.shutdown, defaults: defaults)
        XCTAssertEqual(ControlDisplayClose.remembered(defaults: defaults), .shutdown)
        ControlDisplayClose.remember(nil, defaults: defaults)
        XCTAssertNil(ControlDisplayClose.remembered(defaults: defaults))
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig())
        XCTAssertFalse(ControlDisplayClose.apply(.cancel, session: session, report: { _ in XCTFail("Cancel has no effects") }))
        XCTAssertEqual(session.connectionState, .stopped)
    }
    func testShutdownRefusalKeepsWindowOpenAndReportsOrdinaryStopMessage() {
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig())
        session.attachedToExistingProcess = true
        var message: String?
        XCTAssertFalse(ControlDisplayClose.apply(.shutdown, session: session, report: { message = $0 }))
        XCTAssertEqual(message, "다른 실행 경로에서 시작된 VM은 이 앱이 안전하게 정지할 수 없습니다.")
    }
    func testRenderCloseAlertOffscreen() throws {
        guard let path = ProcessInfo.processInfo.environment["BV_START_CLOSE_RENDER"] else { throw XCTSkip("Opt-in offscreen render") }
        let alert = ControlDisplayClose.alert(choices: [.background, .pause, .shutdown, .cancel])
        alert.layout()
        guard let view = alert.window.contentView,
              let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return XCTFail("No alert image") }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        let data = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        try data.write(to: URL(fileURLWithPath: path))
    }
}
