import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class RebootStopRegressionTests: XCTestCase {
    func testPendingStopIsSentOncePerGuestBootNotPerAgentRestart() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process; session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        _ = try f.stop()
        session.handle(.serviceStart(tMs: 1))
        try await waitForLines(1, f)
        session.handle(.unknown("BVAGENT re-READY host=test t=2"))
        session.handle(.serviceStart(tMs: 2))
        try await Task.sleep(nanoseconds: 30_000_000)
        XCTAssertEqual(try lines(f), ["shutdown.exe /s /t 0"])
        session.handle(.unknown(HvfGuestResetRecord.processRecreation))
        session.handle(.serviceStart(tMs: 3))
        try await waitForLines(2, f)
        session.handle(.serviceStart(tMs: 4))
        try await Task.sleep(nanoseconds: 30_000_000)
        XCTAssertEqual(try lines(f), ["shutdown.exe /s /t 0", "shutdown.exe /s /t 0"])
        XCTAssertNil(f.controller.failure, "A new boot must not hit the writer's run-wide duplicate rejection")
    }

    private func lines(_ f: HvfOwnedRuntimeControllerTests.Fixture) throws -> [String] {
        try String(contentsOfFile: f.base.config.ctlFilePath).split(separator: "\n").map(String.init)
            .filter { $0.hasPrefix("shutdown.exe") }
    }

    private func waitForLines(_ count: Int, _ f: HvfOwnedRuntimeControllerTests.Fixture) async throws {
        let limit = ProcessInfo.processInfo.systemUptime + 1
        while try lines(f).count < count, ProcessInfo.processInfo.systemUptime < limit {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertEqual(try lines(f).count, count)
    }
}
