import Foundation
import XCTest
@testable import BridgeVMControl

/// Actual native CLI, owner endpoint, ledger and runner: a completed stop must not hide a newer pending start.
@MainActor
final class HvfOwnedStopPendingStartTests: XCTestCase {
    private final class Gate: @unchecked Sendable {
        private let lock = NSLock(); private var released = false
        var isOpen: Bool { lock.lock(); defer { lock.unlock() }; return released }
        func open() { lock.lock(); released = true; lock.unlock() }
    }

    func testCompletedStopRecordDoesNotAnswerFreshStopWhileNewStartIsPending() async throws {
        let runner = try HvfOwnedStartRunnerFixture()
        let fixture: HvfOwnedStartCLIFixture
        do { fixture = try HvfOwnedStartCLIFixture(runner: runner) }
        catch { await runner.clean(); throw error }
        let gate = Gate()
        do {
            let (startExit, started) = try await fixture.command("start", pairedRust: false)
            XCTAssertEqual(startExit, 0, "\(started)")
            try await runner.awaitChildren()
            let (stopExit, stopped) = try await fixture.command("stop", pairedRust: false)
            XCTAssertEqual(stopExit, 0, "\(stopped)")
            let firstOperation = try XCTUnwrap(stopped["operationID"] as? String)
            let rawTarget = try XCTUnwrap(stopped["target"] as? [String: Any])
            let target = NativeRuntimeStopTarget(appInstanceID: try XCTUnwrap(rawTarget["appInstanceID"] as? String),
                runToken: try XCTUnwrap(rawTarget["runToken"] as? String), processID: try XCTUnwrap(rawTarget["processID"] as? Int32),
                acceptedConfigurationDigest: try XCTUnwrap(rawTarget["acceptedConfigurationDigest"] as? String))
            let (repeatExit, repeated) = try await fixture.command("stop", pairedRust: false)
            XCTAssertEqual(repeatExit, 0, "Retained completed run still answers a fresh stop: \(repeated)")
            XCTAssertEqual(repeated["operationID"] as? String, firstOperation)

            let session = try XCTUnwrap(runner.session)
            session.ownedStartWorker = { _ in
                while !gate.isOpen { try? await Task.sleep(nanoseconds: 5_000_000) }
                return .failure(.readiness)
            }
            let admitted = try await start(fixture)
            XCTAssertEqual(admitted.observation?.phase, .preparing, "\(admitted)")
            XCTAssertTrue(session.hasPendingOwnedStart)
            XCTAssertEqual(session.lastOwnedExit?.identity.token.uuidString, target.runToken)

            let (pendingExit, pending) = try await fixture.command("stop", pairedRust: false)
            XCTAssertEqual(pendingExit, 1, "Stop must not report run 1 cleanup while a new start is pending: \(pending)")
            XCTAssertEqual(pending["complete"] as? Bool, false)
            XCTAssertEqual(pending["unavailableReason"] as? String, NativeRuntimeStopRefusal.targetUnavailable.rawValue)
            XCTAssertTrue(session.hasPendingOwnedStart, "A refused stop must not disturb the pending start")
            for operation in [NativeRuntimeControlRequest.Operation.stop, .stopStatus] {
                let replay = try await control(fixture, operation, operationID: firstOperation, target: target)
                XCTAssertNil(replay.refusal, "Exact operation replay stays answerable: \(replay)")
                XCTAssertEqual(replay.observation?.operationID, firstOperation)
                XCTAssertEqual(replay.observation?.phase, .completed)
            }

            gate.open()
            let deadline = ProcessInfo.processInfo.systemUptime + 5
            while session.hasPendingOwnedStart, ProcessInfo.processInfo.systemUptime < deadline {
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            XCTAssertFalse(session.hasPendingOwnedStart)
            let (afterExit, after) = try await fixture.command("stop", pairedRust: false)
            XCTAssertEqual(afterExit, 0, "Unspawned failed start leaves run 1 as the retained state: \(after)")
            XCTAssertEqual(after["operationID"] as? String, firstOperation)
            XCTAssertEqual(runner.effects.snapshot.launches, 1)
            await fixture.close()
        } catch { gate.open(); await fixture.close(); throw error }
    }

    func testLedgerJoinsFreshIDOnlyWhileTargetIsRetainedButAlwaysReplaysExactID() throws {
        typealias Fixture = NativeRuntimeControlTestSupport
        let ledger = NativeRuntimeStopLedger(appInstanceID: Fixture.app, library: Fixture.library)
        let request = Fixture.request()
        let ticket = HvfOwnedStopOperation(operationID: UUID(uuidString: request.operationID)!,
            target: .init(token: UUID(uuidString: Fixture.token)!, processID: 123), now: 10, deadline: 202, phase: .guestGrace)
        XCTAssertEqual(try ledger.process(request) { .accepted(ticket) }.disposition, .accepted)
        var admissions = 0
        let fresh = try ledger.process(Fixture.request(), retainsTarget: { false }) {
            admissions += 1; throw NativeRuntimeStopRefusal.targetUnavailable
        }
        XCTAssertEqual(fresh.refusal, .targetUnavailable); XCTAssertEqual(admissions, 1)
        for operation in [NativeRuntimeControlRequest.Operation.stop, .stopStatus] {
            let replay = try ledger.process(Fixture.request(operation, id: request.operationID), retainsTarget: {
                XCTFail("exact replay consulted current state"); return false
            }) { XCTFail("exact replay admitted"); return .accepted(ticket) }
            XCTAssertEqual(replay.observation?.operationID, request.operationID)
        }
        XCTAssertEqual(try ledger.process(Fixture.request(), retainsTarget: { true }) {
            XCTFail("retained target readmitted"); return .accepted(ticket)
        }.observation?.operationID, request.operationID)
    }

    private func start(_ f: HvfOwnedStartCLIFixture) async throws -> NativeRuntimeStartResponse {
        let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
            requestID: UUID().uuidString, library: f.library.identity, vmID: f.runner.config.slug,
            appInstanceID: f.appInstanceID, expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: f.runner.config),
            operationID: UUID().uuidString)
        let root = f.runner.libraryRoot
        return try await Task.detached { try NativeRuntimeStartClient.query(rootURL: root, request: request) }.value
    }

    private func control(_ f: HvfOwnedStartCLIFixture, _ operation: NativeRuntimeControlRequest.Operation,
                         operationID: String, target: NativeRuntimeStopTarget) async throws -> NativeRuntimeControlResponse {
        let request = NativeRuntimeControlRequest(schema: NativeRuntimeControlCodec.requestSchema, operation: operation,
            requestID: UUID().uuidString, library: f.library.identity, vmID: f.runner.config.slug,
            operationID: operationID, target: target)
        let root = f.runner.libraryRoot
        return try await Task.detached { try NativeRuntimeControlClient.query(rootURL: root, request: request) }.value
    }
}
