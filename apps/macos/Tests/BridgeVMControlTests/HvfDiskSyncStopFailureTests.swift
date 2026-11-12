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
        XCTAssertEqual(session.runtimeSupervisionFailureText,
            "Windows를 정상적으로 종료하지 못했으니 Mac의 저장 공간과 디스크 연결을 확인하세요.")
    }
}
