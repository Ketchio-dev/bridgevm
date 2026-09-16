import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeRunnerIntegrationTests: XCTestCase {
    func testOrdinaryTypedAppStopReceivesActualRunnerCleanupAndExit() async throws {
        let fixture = try HvfOwnedRunnerFixture()
        do {
            guard case let .ownedLaunchAccepted(identity) = fixture.session.start(policy: .requireNew) else {
                throw CocoaError(.executableNotLoadable)
            }
            try await fixture.awaitChildren()
            let unrelatedControl = fixture.base.root.appendingPathComponent("unrelated-control")
            try Data("leave unchanged\n".utf8).write(to: unrelatedControl)
            fixture.session.config.ctlFilePath = unrelatedControl.path
            let operationID = UUID()
            guard case let .accepted(operation) = fixture.session.requestOwnedStop(target: identity, operationID: operationID) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            guard case let .existing(repeated) = fixture.session.requestOwnedStop(target: identity, operationID: operationID) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            XCTAssertTrue(operation === repeated)
            try await fixture.waitForStop(operation)
            XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.base.root.appendingPathComponent("guest-command-observed").path))
            XCTAssertEqual(try String(contentsOf: unrelatedControl, encoding: .utf8), "leave unchanged\n")
            XCTAssertEqual(fixture.base.launches, 1)
            await fixture.clean()
        } catch { await fixture.clean(); throw error }
    }

    func testReplacedGuestControlUsesOwnedRunnerCancellationWithoutWritingReplacement() async throws {
        let fixture = try HvfOwnedRunnerFixture()
        do {
            guard case let .ownedLaunchAccepted(identity) = fixture.session.start(policy: .requireNew) else {
                throw CocoaError(.executableNotLoadable)
            }
            try await fixture.awaitChildren()
            let control = URL(fileURLWithPath: fixture.base.config.ctlFilePath)
            try Data("replacement remains unchanged\n".utf8).write(to: control, options: .atomic)
            let operationID = UUID()
            guard case let .accepted(operation) = fixture.session.requestOwnedStop(target: identity, operationID: operationID) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            try await fixture.waitForStop(operation)
            let complete = try XCTUnwrap(operation.observation.supervisorComplete)
            XCTAssertEqual(complete.cause, "stopRequested")
            XCTAssertEqual(complete.outcome, "cancelled")
            XCTAssertEqual(complete.operationID, operationID.uuidString.lowercased())
            XCTAssertEqual(operation.observation.runnerExit?.status, 1)
            XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), "replacement remains unchanged\n")
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.base.root.appendingPathComponent("guest-command-observed").path))
            await fixture.clean()
        } catch { await fixture.clean(); throw error }
    }
}
