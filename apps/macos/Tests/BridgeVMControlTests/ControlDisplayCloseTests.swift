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
    /// A running VM this app started, with Pause available unless `pausable` is false.
    private func running(startedHere: Bool = true, pausable: Bool = true) -> HvfEngineSession {
        let session = HvfEngineSession(config: HvfEngineView.defaultConfig())
        if pausable { session.beginOwnedInputBoot() }
        session.connectionState = .booting
        if startedHere { session.ownedProcessIdentity = HvfOwnedRuntimeIdentity(token: UUID(), processID: 1) }
        return session
    }
    private func plan(_ session: HvfEngineSession, quitting: Bool = false, _ remembered: ControlDisplayCloseChoice?) -> ControlDisplayClosePlan {
        ControlDisplayClosePlan.make(session: session, quitting: quitting, remembered: remembered)
    }
    func testStoppedQuittingPausedAndStoppingCloseWithoutAQuestion() {
        XCTAssertEqual(plan(HvfEngineSession(config: HvfEngineView.defaultConfig()), nil), .close)
        XCTAssertEqual(plan(running(), quitting: true, .shutdown), .close)
        // Paused: Windows stays paused, so neither "Keep running" nor a remembered Pause makes sense.
        for pause in [HvfPauseState.paused, .pausing(sentAt: 0)] {
            let session = running(); session.pause = pause
            for remembered in [nil, ControlDisplayCloseChoice.background, .pause, .shutdown] {
                XCTAssertEqual(plan(session, remembered), .close, "\(pause) \(String(describing: remembered))")
            }
        }
        let stopping = running(); stopping.connectionState = .stopping
        XCTAssertEqual(plan(stopping, .shutdown), .close, "a second close after Shut down closes the window")
    }
    func testRunningAsksOnceOrUsesRememberedChoiceWhenAvailable() {
        XCTAssertEqual(plan(running(), nil), .ask([.background, .pause, .shutdown, .cancel]))
        for choice in [ControlDisplayCloseChoice.background, .pause, .shutdown] {
            XCTAssertEqual(plan(running(), choice), .apply(choice))
        }
        XCTAssertEqual(plan(running(pausable: false), .pause), .ask([.background, .shutdown, .cancel]))
    }
    /// A remembered Shut down the app cannot do (Stop refuses) asks instead of refusing every close.
    func testShutDownIsOfferedOnlyWhenStopCanStopTheVM() {
        XCTAssertEqual(plan(running(startedHere: false), .shutdown), .ask([.background, .pause, .cancel]))
        let starting = HvfEngineSession(config: HvfEngineView.defaultConfig())
        starting.guiStartOperation = HvfGUIStartOperation(configuration: starting.config)
        XCTAssertEqual(plan(starting, .shutdown), .ask([.background, .cancel]))
    }
    /// Shut down keeps the window open, so Windows' own screen stays visible while it closes apps, and
    /// shows no "Stop requested." sheet over it.
    func testShutDownKeepsTheWindowOpenWithoutASheet() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process
        session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        session.ownedStartNow = { f.clock.value }
        let token = f.controller.identity.token
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        try FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        session.connectionState = .booting
        XCTAssertEqual(plan(session, .shutdown), .apply(.shutdown))

        XCTAssertTrue(ControlDisplayClose.apply(.shutdown, session: session, report: { XCTFail("No sheet: \($0)") }))
        XCTAssertFalse(ControlDisplayCloseChoice.shutdown.closesWindow)
        XCTAssertEqual(session.connectionState, .stopping)
    }
    func testAlertHasOnlyAvailableActionsAndRememberCheckbox() {
        let alert = ControlDisplayClose.alert(choices: [.background, .shutdown, .cancel])
        XCTAssertEqual(alert.buttons.map(\.title), ["Keep running in the background", "Shut down Windows", "Cancel"])
        XCTAssertEqual(alert.buttons.last?.keyEquivalent, "\u{1b}")
        XCTAssertEqual(alert.suppressionButton?.title, "Remember my choice")
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
        XCTAssertEqual(message, "Stop the VM from the app or tool that started it.")
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
