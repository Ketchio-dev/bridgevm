import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDisplayDriverNoticeTests: XCTestCase {
    func testReadyGuestWithoutDriverShowsInstall() {
        XCTAssertEqual(shown(), .install)
        XCTAssertEqual(shown(installed: nil), .install)
    }

    func testInstalledDriverHidesNotice() {
        XCTAssertNil(shown(installed: true))
        XCTAssertNil(shown(installed: true, restartRequired: true))
    }

    func testAgentNotReadyHidesNotice() {
        XCTAssertNil(shown(agentReady: false))
        XCTAssertNil(shown(agentReady: false, restartRequired: true))
    }

    func testExperimental3DDoesNotNeedThe2DDriver() {
        XCTAssertNil(shown(experimental3D: true))
    }

    func testDismissalHidesBothStates() {
        XCTAssertNil(shown(dismissed: true))
        XCTAssertNil(shown(dismissed: true, restartRequired: true))
    }

    func testInstalledAndStagedSwitchToRestartThroughExistingInstallPath() async throws {
        for status in ["installed", "staged", "failed"] {
            let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
            defer { fixture.clean() }
            let saved = fixture.config("notice-\(status)"); fixture.save(saved)
            let library = fixture.library(probe: probe)
            let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
            session.connectionState = .connected(host: "guest")
            session.serviceStarted = true
            XCTAssertEqual(session.displayDriverNotice, .install)
            _ = await withCheckedContinuation { (done: CheckedContinuation<HvfDisplayDriverInstall.Outcome, Never>) in
                HvfDisplayDriverInstall.start(runner: { _, _ in ("BVDRIVER result=\(status) detail=x", 0) },
                    onInstalled: { _ in HvfDisplayDriverFlag.record(session) }) { done.resume(returning: $0) }
            }
            XCTAssertEqual(session.displayDriverNotice, status == "failed" ? .install : .restart)
            session.displayDriverNoticeDismissed = true
            XCTAssertNil(session.displayDriverNotice)
            session.markStopped()
            XCTAssertTrue(session.displayDriverNoticeDismissed, "Stopping must not undo this app session's dismissal")
            XCTAssertFalse(session.displayDriverRestartRequired)
        }
    }

    /// Synthetic runner protocol with a real retained shell, not a Windows or GUI test.
    func testRestartWaitsForGracefulStopAndReapBeforeStartingWithTheSavedDevice() async throws {
        let owned = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { owned.clean() }
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config("notice-restart"); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        session.guiStartWorker = { input in
            XCTAssertEqual(input.policy, .requireNew)
            XCTAssertTrue(input.config.virtioGpu2d)
            return HvfGUIStartWorkerOutput(result: .refused("fixture: launch suppressed"))
        }
        session.process = owned.process
        session.ownedProcessIdentity = owned.controller.identity
        session.ownedController = owned.controller
        let token = owned.controller.identity.token
        owned.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        owned.controller.receive(.message(try owned.event("ready", sequence: 1)))
        owned.controller.guestServiceReady()
        HvfDisplayDriverFlag.record(session)

        let restart = try XCTUnwrap(session.restartForDisplayDriver())
        let deadline = Date().addingTimeInterval(2)
        while owned.controller.operation == nil, Date() < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        let ctl = URL(fileURLWithPath: owned.base.config.ctlFilePath)
        while (try? String(contentsOf: ctl, encoding: .utf8))?.contains("shutdown.exe /s /t 0") != true,
              Date() < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(try String(contentsOf: ctl, encoding: .utf8), "shutdown.exe /s /t 0\n")
        XCTAssertNil(session.guiStartOperation, "Never start while Windows is shutting down")
        XCTAssertNil(session.restartForDisplayDriver(), "Do not queue another restart")
        owned.controller.receive(.message(try owned.event("complete-not-admitted", sequence: 2, success: true)))
        try await owned.exit()
        XCTAssertNil(session.guiStartOperation, "Process exit alone is not full cleanup")
        owned.controller.receive(.eof)
        await restart.value
        let start = try XCTUnwrap(session.guiStartOperation)
        XCTAssertTrue(start.configuration.virtioGpu2d)
        XCTAssertEqual(start.configuration.libraryContext?.config.virtioDisplayDriverInstalled, true)
        XCTAssertFalse(session.displayDriverRestarting)
        // Let the injected no-launch worker finish before fixture cleanup.
        while session.hasPendingGUIStart { try await Task.sleep(nanoseconds: 5_000_000) }
    }

    private func shown(agentReady: Bool = true, installed: Bool? = false, experimental3D: Bool = false,
                       dismissed: Bool = false, restartRequired: Bool = false) -> HvfDisplayDriverNotice? {
        HvfDisplayDriverNotice.shown(agentReady: agentReady, installed: installed,
            experimental3D: experimental3D, dismissed: dismissed, restartRequired: restartRequired)
    }
}
