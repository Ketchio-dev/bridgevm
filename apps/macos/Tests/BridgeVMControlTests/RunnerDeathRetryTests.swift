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
        XCTAssertEqual(session.guiStartFailureText, "Windows stopped unexpectedly, so wait a moment before starting it again.")
        XCTAssertTrue(session.hasActiveRuntimeWork, "Unconfirmed cleanup still blocks unrelated mutations")
        session.guiStartWorker = { _ in .init(result: .refused("The previous Windows session is still closing its files. Try starting it again in a moment.")) }
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("The same session must allow retry to reach the lease check")
        }
        let limit = ProcessInfo.processInfo.systemUptime + 2
        while ticket.workerPending, ProcessInfo.processInfo.systemUptime < limit { try await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(session.guiStartFailureText, "The previous Windows session is still closing its files. Try starting it again in a moment.")
        XCTAssertTrue(f.controller.mayHaveOwnedWork, "Retry is not a fabricated cleanup receipt")
    }
}
