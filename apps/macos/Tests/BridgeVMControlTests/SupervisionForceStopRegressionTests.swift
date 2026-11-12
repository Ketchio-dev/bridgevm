import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class SupervisionForceStopRegressionTests: XCTestCase {
    func testLostSupervisionOffersOwnedRunnerCancellationWithoutCleanupProof() async throws {
        for invalid in [false, true] {
            let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
            let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
                processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
            session.process = f.process; session.ownedProcessIdentity = f.controller.identity
            session.ownedController = f.controller
            f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: f.controller.identity.token) }
            defer { f.controller.onChange = nil }
            f.controller.receive(.message(try f.event("ready", sequence: 1)))
            if invalid { _ = try f.stop() }
            if invalid { f.controller.receive(.message(try f.event("ready", sequence: 1))) }
            else { f.controller.receive(.failed(.channelClosed)) }
            XCTAssertTrue(f.process.isRunning)
            XCTAssertEqual(session.connectionState, .timedOut)
            XCTAssertEqual(session.runtimeSupervisionFailureText,
                "실행 중인 Windows와 연결이 끊겼으니 강제 중단으로 Windows를 멈추세요.")
            let target = try XCTUnwrap(session.forceStopTarget, "Lost supervision must still offer Force Stop")
            let wrong = HvfForceStopTarget(identity: .init(token: UUID(), processID: f.process.processIdentifier),
                operationID: target.operationID)
            XCTAssertEqual(session.requestForceStop(wrong), .refused(.targetChanged))
            XCTAssertEqual(session.requestForceStop(target), .requested)
            XCTAssertNil(session.forceStopTarget)
            let limit = ProcessInfo.processInfo.systemUptime + 2
            while f.process.isRunning, ProcessInfo.processInfo.systemUptime < limit {
                try await Task.sleep(nanoseconds: 5_000_000)
            }
            XCTAssertFalse(f.process.isRunning, "Force Stop must actually cancel the retained runner")
            XCTAssertTrue(f.controller.mayHaveOwnedWork)
            XCTAssertFalse(try XCTUnwrap(f.controller.operation).observation.isComplete)
        }
    }
}
