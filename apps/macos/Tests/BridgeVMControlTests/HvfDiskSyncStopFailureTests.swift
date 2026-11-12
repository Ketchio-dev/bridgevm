import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDiskSyncStopFailureTests: XCTestCase {
    @MainActor
    func testFailedFinalPersistenceDoesNotReadAsNormalStop() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        session.ownedController = f.controller
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        let complete = try f.event("complete-not-admitted", sequence: 2, success: true)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(complete)) as? [String: Any])
        var payload = try XCTUnwrap(object["complete"] as? [String: Any])
        payload["outcome"] = "failed"
        payload["failureCode"] = "helperFailed"
        object["complete"] = payload
        let failed = try JSONDecoder().decode(HvfOwnedRuntimeEvent.self,
            from: JSONSerialization.data(withJSONObject: object))
        f.controller.receive(.message(failed))
        try await f.exit()
        f.controller.receive(.eof)
        XCTAssertEqual(ticket.observation.failure, .cleanupFailed)
        XCTAssertFalse(ticket.observation.isComplete)
        XCTAssertNil(session.runtimeSupervisionFailureText,
            "helperFailed alone says nothing about storage")
        let start = HvfGUIStartOperation(configuration: f.base.config)
        start.finish(.failed(.processLaunch, "specific start failure"))
        session.guiStartOperation = start
        XCTAssertNil(session.runtimeSupervisionFailureText, "must not mask the specific start failure")
        XCTAssertEqual(session.guiStartFailureText, "VM을 시작하지 못했습니다: specific start failure")
        session.guiStartOperation = nil
        for event in BvAgentEvent.parse(lines: ["BVSTORAGE final-save-failed"]) { session.handle(event) }
        XCTAssertEqual(session.runtimeSupervisionFailureText,
            "Windows를 종료하면서 디스크를 Mac에 안전하게 저장하지 못했으니 Mac의 저장 공간과 디스크 연결을 확인하세요.")
        session.guiStartOperation = start
        XCTAssertNil(session.runtimeSupervisionFailureText, "even a storage event cannot mask a specific startup error")
        session.guiStartOperation = nil
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertNil(session.runtimeSupervisionFailureText, "a new run clears the storage cause")
    }

    @MainActor
    func testFailedStartReceiptKeepsSpecificStartSentence() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        session.ownedController = f.controller
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 1)))
        let start = HvfGUIStartOperation(configuration: f.base.config)
        start.finish(.failed(.processLaunch, "disk in use"))
        session.guiStartOperation = start
        XCTAssertNotNil(f.controller.ledger.complete?.failureCode)
        XCTAssertNil(session.runtimeSupervisionFailureText)
        XCTAssertEqual(session.guiStartFailureText, "VM을 시작하지 못했습니다: disk in use")
    }

    @MainActor
    func testFinalSaveDiagnosticDoesNotMaskRunnerDeath() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        session.ownedController = f.controller
        session.handle(.diskFinalSaveFailed)
        try await f.exit()
        XCTAssertTrue(session.runnerDiedWithoutCleanup)
        XCTAssertNil(session.runtimeSupervisionFailureText)
        XCTAssertEqual(session.guiStartFailureText, "Windows가 예기치 않게 종료되었으니 잠시 후 다시 시작하세요.")
    }

    @MainActor
    func testStopDrainsFinalStorageDiagnosticBeforePollingEnds() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        let log = URL(fileURLWithPath: f.base.config.evidenceDir).appendingPathComponent("run.log")
        try FileManager.default.createDirectory(at: log.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("BVSTORAGE final-save-failed\n".utf8).write(to: log)
        session.markStopped()
        XCTAssertTrue(session.diskSyncNotice.finalSaveFailed)
        XCTAssertNotNil(session.runtimeSupervisionFailureText)
    }
}
