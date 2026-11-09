import Foundation
import XCTest
@testable import BridgeVMControl

/// Pause and Resume as the app drives them: input.ctl `PAUSE`/`RESUME`, the probe's `BVPAUSE` lines.
@MainActor
final class HvfGuestPauseTests: XCTestCase {
    private typealias Owned = HvfOwnedRuntimeControllerTests.Fixture

    /// Keys and text do not reach a paused guest, from the press of Pause until the probe says the
    /// guest runs again.
    func testInputIsIgnoredFromPauseUntilTheGuestRunsAgain() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            XCTAssertTrue(session.sendKey("enter"))
            XCTAssertNotEqual(session.sendText("x"), .refused)
            XCTAssertEqual(session.requestPause(), .requested)
            XCTAssertFalse(session.sendKey("enter"), "ignored while the pause is under way")
            try fixture.appendLog("BVPAUSE vtimer offset: set=0x0 get=0x0 read=0x0 wanted=0x0\nBVPAUSE paused\n")
            session.poll()
            XCTAssertEqual(session.pause, .paused)
            XCTAssertFalse(session.sendKey("esc"), "ignored while paused")
            XCTAssertEqual(session.sendText("x"), .refused)
            XCTAssertEqual(session.requestResume(), .requested)
            try fixture.appendLog("BVPAUSE resumed paused_ms=60000 vtimer_offset=0x2b\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            XCTAssertTrue(session.sendKey("enter"), "the guest takes input again")
            XCTAssertNotEqual(session.sendText("y"), .refused)
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("PAUSE\nRESUME\n".utf8))
        }
    }

    /// Review on 3d9f317f: a Pause and a Resume the probe reads together never pause the VM. The probe
    /// answers the Resume for the running VM (`paused_ms=0`), and the app shows it running and takes
    /// input again. A Restart while the pause is under way resumes first and ends the same way.
    func testAPauseAndResumeReadTogetherEndRunning() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            XCTAssertEqual(session.requestPause(), .requested)
            XCTAssertEqual(session.requestResume(), .requested)
            try fixture.appendLog("BVPAUSE resumed paused_ms=0 vtimer_offset=0x0\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            XCTAssertTrue(session.sendKey("enter"))

            XCTAssertEqual(session.requestPause(), .requested)
            XCTAssertEqual(session.requestRestart(), .requested(.windowsAsked))
            try fixture.appendLog("BVPAUSE resumed paused_ms=0 vtimer_offset=0x0\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("PAUSE\nRESUME\nPAUSE\nRESUME\n".utf8))
        }
    }

    /// A Pause the probe refuses, or one nothing answers, leaves the VM running and says so; Pause
    /// works again afterwards.
    func testARefusedOrUnansweredPauseLeavesTheVMRunning() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var now: TimeInterval = 100
            session.ownedStartNow = { now }
            session.beginOwnedInputBoot()
            session.connectionState = .booting
            XCTAssertEqual(session.requestPause(), .requested)
            try fixture.appendLog("BVPAUSE refused: a vCPU did not stop within 2s; the VM runs on\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            XCTAssertNotNil(session.pauseNotice)

            XCTAssertEqual(session.requestPause(), .requested)
            XCTAssertNil(session.pauseNotice)
            now += HvfEngineSession.pauseAnswerTimeout
            session.poll()
            XCTAssertEqual(session.pause, .running, "no BVPAUSE answer: the VM still runs")
            XCTAssertNotNil(session.pauseNotice)
            XCTAssertEqual(session.requestPause(), .requested)
        }
    }

    /// A full Mac disk pauses the VM (`BVSTORAGE full …`, then `BVPAUSE paused`) and the app says so in
    /// one sentence. A Resume while the disk is still full is answered the same way and stays paused;
    /// once there is space it runs again. A plain Pause says nothing.
    func testAFullMacDiskPauseSaysSoAndAPlainPauseDoesNot() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.beginOwnedInputBoot()
            session.connectionState = .booting
            let full = "BVSTORAGE full errno=28 free=0 path=/Volumes/Mac SSD\nBVPAUSE paused\n"
            try fixture.appendLog(full)
            session.poll()
            XCTAssertEqual(session.pause, .paused)
            XCTAssertEqual(session.pauseNotice, HvfEngineSession.diskFullNotice)

            XCTAssertEqual(session.requestResume(), .requested)
            XCTAssertNil(session.pauseNotice)
            try fixture.appendLog(full)
            session.poll()
            XCTAssertEqual(session.pause, .paused, "still full: still paused")
            XCTAssertEqual(session.pauseNotice, HvfEngineSession.diskFullNotice)

            XCTAssertEqual(session.requestResume(), .requested)
            try fixture.appendLog("BVPAUSE resumed paused_ms=9000 vtimer_offset=0x2b\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            XCTAssertNil(session.pauseNotice)

            XCTAssertEqual(session.requestPause(), .requested)
            try fixture.appendLog("BVPAUSE paused\n")
            session.poll()
            XCTAssertEqual(session.pause, .paused)
            XCTAssertNil(session.pauseNotice, "a plain pause says nothing")
        }
    }

    /// A guest reset (RESET while paused, or Windows restarting) ends the pause: the probe resumes first.
    func testAGuestResetEndsThePause() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.beginOwnedInputBoot()
            session.connectionState = .booting
            XCTAssertEqual(session.requestPause(), .requested)
            try fixture.appendLog("BVPAUSE paused\n" + HvfGuestResetRecord.processRecreation + "\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
        }
    }

    /// A paused Windows cannot shut down: Stop (and quit, which stops the same way) resumes it before
    /// asking Windows to shut down (the power button press since #513).
    func testStopWhilePausedResumesTheGuestFirst() throws {
        let f = try Owned(); defer { f.clean() }
        let session = bind(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        session.beginOwnedInputBoot()
        session.connectionState = .booting
        XCTAssertEqual(session.requestPause(), .requested)
        try append("BVPAUSE paused\n", to: f)
        session.poll()
        XCTAssertEqual(session.pause, .paused)

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        // RESUME before the Stop's power button press (#513): Windows has to run to shut down.
        XCTAssertEqual(try input(f), "PAUSE\nRESUME\nPOWERBUTTON\n")
        XCTAssertEqual(session.pause, .resuming(sentAt: f.clock.value))
    }

    /// An app that attaches to a helper the previous app left paused resumes it; one that is not
    /// paused gets nothing.
    func testReattachingResumesAPausedVM() throws {
        let f = try Owned(); defer { f.clean() }
        try FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        let running = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in true }, vtpmKeyProvider: f.base.keys)
        try append("BVPAUSE paused\nBVPAUSE resumed paused_ms=5000 vtimer_offset=0x7270e00\n", to: f)
        XCTAssertTrue(running.attachToRunningVM(reportRefusal: false))
        XCTAssertEqual(running.pause, .running)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.base.config.evidenceDir + "/input.ctl"))
        running.markStopped()

        let paused = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in true }, vtpmKeyProvider: f.base.keys)
        try append("BVPAUSE paused\n", to: f)
        XCTAssertTrue(paused.attachToRunningVM(reportRefusal: false))
        XCTAssertEqual(try input(f), "RESUME\n")
        guard case .resuming = paused.pause else { return XCTFail("\(paused.pause)") }
        paused.markStopped()
    }

    private func bind(_ f: Owned) -> HvfEngineSession {
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process
        session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        session.ownedStartNow = { f.clock.value }
        let token = f.controller.identity.token
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        try? FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        return session
    }

    private func input(_ f: Owned) throws -> String {
        try String(contentsOfFile: f.base.config.evidenceDir + "/input.ctl", encoding: .utf8)
    }

    private func append(_ text: String, to f: Owned) throws {
        let log = URL(fileURLWithPath: f.base.config.evidenceDir).appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: log.path) { FileManager.default.createFile(atPath: log.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
