import Foundation
import XCTest
@testable import BridgeVMControl

/// The agent notice, driven through run.log and poll() as a running VM drives it: no SERVICE start two
/// minutes after Windows started (in a VM whose agent has started before), or the probe's overdue
/// line. It clears once the agent answers, and stays off while the VM is paused, stopping or restarting.
@MainActor
final class HvfAgentSilenceNoticeTests: XCTestCase {
    private final class Clock { var value: TimeInterval = 1_000 }
    private let ready = ["BVAGENT READY host=BRIDGEVM t=9000", "BVAGENT SERVICE start t=9500"]

    private func session(_ clock: Clock, agentSeen: Bool = true) throws -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-agent-silence-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        session.ownedStartNow = { clock.value }
        if agentSeen { FileManager.default.createFile(atPath: session.agentSeenMarker.path, contents: nil) }
        session.beginOwnedInputBoot()
        session.connectionState = .booting
        return session
    }

    /// Appends run.log lines and polls once, as the session's timer does.
    private func run(_ session: HvfEngineSession, _ lines: [String] = []) throws {
        let log = URL(fileURLWithPath: session.config.evidenceDir).appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: log.path) { FileManager.default.createFile(atPath: log.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(lines.map { $0 + "\n" }.joined().utf8))
        session.poll()
    }

    func testNoAgentTwoMinutesAfterWindowsStartedShowsTheNoticeUntilItStarts() throws {
        let clock = Clock()
        let session = try session(clock)
        try run(session)
        clock.value += HvfAgentSilenceNotice.readyTimeout - 1
        try run(session)
        XCTAssertNil(session.agentSilenceNotice.text, "Windows is still starting, or the user is logging on")
        clock.value += 1
        try run(session)
        XCTAssertEqual(session.agentSilenceNotice.shown, .notConnected)
        XCTAssertEqual(session.agentSilenceNotice.text,
            "Windows 에이전트가 아직 연결되지 않아 클립보드·공유 폴더·화면 크기 맞춤이 동작하지 않습니다. Windows에 로그인한 뒤에도 계속되면 Windows를 다시 시작하세요.")
        try run(session, ready)
        XCTAssertNil(session.agentSilenceNotice.shown, "the agent is there: the notice clears by itself")
    }

    /// A VM whose agent never started (Windows setup's first logon still ahead, or a disk imported
    /// without the agent) stays quiet; once the agent has started, a later boot without it is noticed.
    func testAVMWhoseAgentNeverStartedStaysQuiet() throws {
        let clock = Clock()
        let session = try session(clock, agentSeen: false)
        try run(session)
        clock.value += 30 * 60
        try run(session)
        XCTAssertNil(session.agentSilenceNotice.shown)
        try run(session, ready)
        XCTAssertTrue(FileManager.default.fileExists(atPath: session.agentSeenMarker.path))
        try run(session, [HvfGuestResetRecord.processRecreation])
        XCTAssertFalse(session.serviceStarted)
        clock.value += HvfAgentSilenceNotice.readyTimeout
        try run(session)
        XCTAssertEqual(session.agentSilenceNotice.shown, .notConnected, "the next boot's agent did not come back")
    }

    /// The probe repeats the overdue line every 20 s while a request waits; once the probe has run on
    /// 30 s without one, or the agent says READY again, the notice clears.
    func testAnOldImportedVMWithoutAnAgentOffersToolsButISOFirstLogonDoesNot() throws {
        let clock = Clock()
        let session = try session(clock, agentSeen: false)
        let root = URL(fileURLWithPath: session.config.evidenceDir)
        let vm = VMConfig(name: "imported", displayName: "imported", backendKind: "hvf-engine",
            bundlePath: root.path, runnerPath: "", launchSpecPath: "", handoffPath: "",
            sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "imported", displayWidth: 1280, displayHeight: 800)
        session.config.libraryContext = HvfLibraryLaunchContext(config: vm, rootURL: root)
        try run(session)
        clock.value += 121
        try run(session)
        XCTAssertEqual(session.agentSilenceNotice.text,
            "게스트 도구가 설치되지 않아 클립보드와 공유 폴더를 사용할 수 없으니 ‘게스트 도구 설치’를 누르세요.")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        for name in [HvfWindowsInstallRequest.fileName, HvfWindowsInstallRequest.doneFileName] {
            try Data().write(to: root.appendingPathComponent(name))
            session.agentSilenceNotice.reset()
            try run(session)
            clock.value += 121
            try run(session)
            XCTAssertNil(session.agentSilenceNotice.text, "our own ISO install stays quiet through first logon")
            try FileManager.default.removeItem(at: root.appendingPathComponent(name))
        }
        var pending = vm
        pending.installPending = true
        session.config.libraryContext = HvfLibraryLaunchContext(config: pending, rootURL: root)
        XCTAssertFalse(HvfGuestTools.eligible(session.config))
        session.config.libraryContext = HvfLibraryLaunchContext(config: vm, rootURL: root)
        let image = try XCTUnwrap(HvfGuestTools.image(session.config))
        try Data(repeating: 0, count: 512).write(to: image)
        XCTAssertEqual(HvfGuestTools.attachment(session.config), image.path)
        try run(session, ["BVGUESTTOOLS failed: missing file"])
        XCTAssertEqual(session.guestToolsMessage, HvfGuestTools.failed)
        XCTAssertEqual(session.connectionState, .booting, "attachment failure does not stop Windows")
        try run(session, ["BVGUESTTOOLS attached"])
        XCTAssertEqual(session.guestToolsMessage, HvfGuestTools.attached)
        try run(session, ready)
        XCTAssertNil(session.agentSilenceNotice.text)
        XCTAssertNil(session.guestToolsMessage)
        XCTAssertNil(HvfGuestTools.attachment(session.config), "later starts omit the tools image")
        try run(session, [HvfGuestResetRecord.processRecreation])
        clock.value += 121
        try run(session)
        XCTAssertEqual(session.agentSilenceNotice.shown, .notConnected, "an imported VM whose installed agent breaks keeps the old notice")
    }

    func testAnOverdueRequestShowsUntilTheProbeRunsOnWithoutOne() throws {
        let session = try session(Clock())
        try run(session, ready)
        XCTAssertNil(session.agentSilenceNotice.shown)
        try run(session, ["BVAGENT SERVICE overdue clip-poll awaiting-reply=true t=50000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding)
        XCTAssertEqual(session.agentSilenceNotice.text,
            "Windows 에이전트가 응답하지 않아 클립보드·공유 폴더·화면 크기 맞춤이 잠시 동작하지 않습니다. 계속되면 Windows를 다시 시작하세요.")
        try run(session, ["BVAGENT SERVICE alive t=60000", "BVAGENT SERVICE overdue ping awaiting-reply=true t=70000",
                          "BVAGENT SERVICE alive t=90000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding, "still overdue")
        try run(session, ["BVAGENT SERVICE alive t=120000"])
        XCTAssertNil(session.agentSilenceNotice.shown, "answered: no overdue line for 30 s")
        try run(session, ["BVAGENT SERVICE overdue clip-caps awaiting-reply=true t=130000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding)
        try run(session, ["BVAGENT re-READY host=BRIDGEVM t=131000"])
        XCTAssertNil(session.agentSilenceNotice.shown, "the agent restarted and spoke")
    }

    /// Review on f8c363f3: the probe calls any request overdue after 20 s, but a guest command (a
    /// display-driver install runs minutes) or a clipboard image read may take longer from a healthy
    /// agent. Only the keepalive requests count; a slow one neither shows the notice nor clears it.
    func testOnlyAnOverdueKeepaliveShowsTheNotice() throws {
        let session = try session(Clock())
        try run(session, ready + ["BVAGENT SERVICE overdue ctl awaiting-reply=true t=30000",
                                  "BVAGENT SERVICE overdue clip-image-pull awaiting-reply=true t=50000",
                                  "BVAGENT SERVICE timeout ctl t=60000"])
        XCTAssertNil(session.agentSilenceNotice.shown, "a slow guest command or clipboard image read")
        for line in ["BVAGENT SERVICE overdue ctl awaiting-reply=true t=70000",
                     "BVAGENT SERVICE overdue clip-files-pull awaiting-reply=true t=90000",
                     "BVAGENT SERVICE overdue agent-update awaiting-reply=true t=110000"] {
            try run(session, [line])
            XCTAssertNil(session.agentSilenceNotice.shown, line)
        }
        try run(session, ["BVAGENT SERVICE overdue ping awaiting-reply=true t=120000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding, "the keepalive got no answer")
        try run(session, ["BVAGENT SERVICE overdue ctl awaiting-reply=true t=160000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding, "a slow request's line clears nothing")
        try run(session, ["BVAGENT SERVICE start t=161000"])
        XCTAssertNil(session.agentSilenceNotice.shown, "the agent spoke")
    }

    /// An app that attaches reads run.log from its start: an overdue line long answered stays quiet.
    func testAnOldOverdueReadOnAttachStaysQuiet() throws {
        let session = try session(Clock())
        try run(session, ready + ["BVAGENT SERVICE overdue clip-poll awaiting-reply=true t=20000",
                                  "BVAGENT SERVICE alive t=60000"])
        XCTAssertNil(session.agentSilenceNotice.shown)
    }

    func testNothingShowsWhilePausedStoppingOrRestarting() throws {
        let clock = Clock()
        let session = try session(clock)
        try run(session, ready + ["BVAGENT SERVICE overdue clip-poll awaiting-reply=true t=50000"])
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding)
        session.pause = .paused
        try run(session)
        XCTAssertNil(session.agentSilenceNotice.shown, "paused")
        session.pause = .running
        try run(session)
        XCTAssertEqual(session.agentSilenceNotice.shown, .notResponding)
        session.restartRequest = HvfPendingRestart(kind: .windowsAsked, boot: session.guestBoot, sentAt: clock.value)
        session.refreshAgentSilence()
        XCTAssertNil(session.agentSilenceNotice.shown, "Windows is restarting on purpose")
        session.restartRequest = nil
        session.connectionState = .stopping
        session.refreshAgentSilence()
        XCTAssertNil(session.agentSilenceNotice.shown, "stopping")

        // The wait for the agent starts again after a pause.
        let waiting = try self.session(clock)
        try run(waiting)
        clock.value += 100
        try run(waiting)
        waiting.pause = .paused
        try run(waiting)
        clock.value += 600
        waiting.pause = .running
        try run(waiting)
        clock.value += 100
        try run(waiting)
        XCTAssertNil(waiting.agentSilenceNotice.shown, "100 s of running since the pause")
        clock.value += 20
        try run(waiting)
        XCTAssertEqual(waiting.agentSilenceNotice.shown, .notConnected)
        waiting.markStopped()
        XCTAssertNil(waiting.agentSilenceNotice.shown, "the run ended")
    }

    /// Closing the strip over the display hides only that strip, until the next notice.
    func testClosingTheStripKeepsTheDashboardNotice() throws {
        let session = try session(Clock())
        let notice = session.agentSilenceNotice
        try run(session, ready + ["BVAGENT SERVICE overdue clip-poll awaiting-reply=true t=50000"])
        notice.dismissInWindow()
        XCTAssertTrue(notice.dismissedInWindow)
        XCTAssertNotNil(notice.text, "the dashboard still says it")
        try run(session, ["BVAGENT SERVICE alive t=90000"])
        XCTAssertNil(notice.text)
        XCTAssertFalse(notice.dismissedInWindow)
        try run(session, ["BVAGENT SERVICE overdue clip-poll awaiting-reply=true t=100000"])
        XCTAssertFalse(notice.dismissedInWindow, "a new notice shows over the display again")
    }
}
