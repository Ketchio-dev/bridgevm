import Foundation
import XCTest
@testable import BridgeVMControl

/// A VM stopped through its wrapper process (no owned controller, as after BridgeVM reattached to it)
/// gets 180 s to shut down before the wrapper is ended. That grace runs on uptime, which stops while
/// the Mac sleeps: on the wall clock, closing the lid during the shutdown ended Windows on wake.
@MainActor
final class HvfAttachedStopDeadlineTests: XCTestCase {
    func testShutdownGraceCountsUptimeNotTheWallClock() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        var uptime: TimeInterval = 1_000
        session.ownedStartNow = { uptime }
        let wrapper = Process()
        wrapper.executableURL = URL(fileURLWithPath: "/bin/sleep"); wrapper.arguments = ["30"]
        try wrapper.run()
        defer { if wrapper.isRunning { wrapper.terminate() }; wrapper.waitUntilExit() }
        let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: wrapper.processIdentifier)
        session.process = wrapper; session.ownedProcessIdentity = identity
        defer { session.timer?.invalidate(); session.timer = nil }

        XCTAssertEqual(session.stopOwned(expectedToken: identity.token),
                       .requested(deadline: Date(timeIntervalSinceReferenceDate: 1_180)))
        uptime = 1_179; session.poll()
        XCTAssertTrue(wrapper.isRunning, "still inside the 180 s grace")
        uptime = 1_181; session.poll()
        wrapper.waitUntilExit()
        XCTAssertEqual(wrapper.terminationReason, .uncaughtSignal, "the wrapper is ended once the grace is used up")
    }
}
