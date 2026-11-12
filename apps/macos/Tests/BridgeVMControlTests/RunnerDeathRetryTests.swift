import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class RunnerDeathRetryTests: XCTestCase {
    func testDeadRunnerHasActionableMessageAndSameSessionCanRetry() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = f.base.session()
        session.process = f.process; session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: f.controller.identity.token) }
        defer { f.controller.onChange = nil }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertEqual(session.connectionState, .timedOut, "Never claim a clean stop without a receipt")
        XCTAssertEqual(session.guiStartFailureText, "Windows가 예기치 않게 종료되었으니 잠시 후 다시 시작하세요.")
        XCTAssertTrue(session.hasActiveRuntimeWork, "Unconfirmed cleanup still blocks unrelated mutations")
        session.guiStartWorker = { _ in .init(result: .refused("이전 Windows가 파일을 정리하고 있으니 잠시 후 다시 시작하세요.")) }
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("The same session must allow retry to reach the lease check")
        }
        let limit = ProcessInfo.processInfo.systemUptime + 2
        while ticket.workerPending, ProcessInfo.processInfo.systemUptime < limit { try await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(session.guiStartFailureText, "이전 Windows가 파일을 정리하고 있으니 잠시 후 다시 시작하세요.")
        XCTAssertTrue(f.controller.mayHaveOwnedWork, "Retry is not a fabricated cleanup receipt")
    }
}
