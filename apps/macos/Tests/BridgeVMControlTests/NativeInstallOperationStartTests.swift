import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOperationStartTests: XCTestCase {
    func testStartOwnsSessionBeforeValidatingPublication() async throws {
        let fixture = NativeInstallOwnedAdoptionFixture()
        defer { fixture.clean() }
        let operation = NativeInstallOperation(operationID: UUID(),
            expectedSavedConfigurationDigest: String(repeating: "a", count: 64))
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            validate: { [probe = fixture.validator] in probe.validate($0) }, schedule: fixture.queue.enqueue,
            execution: .init(process: fixture.process), recovery: .init(inspect: { _ in .fresh }))
        fixture.session = session
        var publications = 0
        let subscription = session.$stage.sink { stage in
            guard stage == .validating else { return }
            publications += 1
            XCTAssertTrue(operation.reservesWork)
            do {
                let observed = try operation.observation()
                XCTAssertEqual(observed.phase, .validating)
                XCTAssertTrue(observed.sessionRunning)
                XCTAssertTrue(observed.canCancel)
            } catch { XCTFail("Starting-session observation is invalid: \(error)") }
        }
        defer { subscription.cancel() }
        XCTAssertTrue(operation.start(session))
        await fixture.waitForAcknowledgment()
        XCTAssertEqual(publications, 1)
        XCTAssertEqual(fixture.queue.jobs.count, 1)
    }

    func testObservationAndCancellationDuringFreshAndRetryValidatingPublication() async throws {
        for retry in [false, true] {
            let fixture = NativeInstallOwnedAdoptionFixture()
            defer { fixture.clean() }
            let operation = NativeInstallOperation(operationID: UUID(),
                expectedSavedConfigurationDigest: String(repeating: "a", count: 64))
            let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
                validate: { [probe = fixture.validator] in probe.validate($0) }, schedule: fixture.queue.enqueue,
                execution: .init(process: fixture.process), recovery: .init(inspect: { _ in .fresh }))
            fixture.session = session
            if retry { session.finish(.failed("previous synthetic attempt")) }
            var publications = 0
            let subscription = session.$stage.sink { stage in
                guard stage == .validating else { return }
                publications += 1
                do {
                    XCTAssertEqual(try operation.observation().phase, .validating)
                    XCTAssertTrue(operation.cancel())
                    XCTAssertEqual(try operation.observation().phase, .cancelling)
                } catch { XCTFail("Retry observation is invalid: \(error)") }
            }
            defer { subscription.cancel() }
            XCTAssertTrue(operation.start(session))
            XCTAssertEqual(try operation.observation().phase, .cancelling)
            await fixture.waitForAcknowledgment()
            XCTAssertEqual(publications, 1)
            XCTAssertEqual(try operation.observation().phase, .cancelled)
            XCTAssertEqual(fixture.validator.snapshot.calls, 0)
            XCTAssertTrue(fixture.queue.jobs.isEmpty)
        }
    }

    func testAdoptedCommitRemainsNonCancellable() throws {
        let operation = NativeInstallOperation(operationID: UUID(),
            expectedSavedConfigurationDigest: String(repeating: "a", count: 64))
        let process = NativeInstallOwnedAdoptionProcess()
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
            execution: .init(process: process))
        session.attemptActive = true
        session.commitStarted = true
        session.transition(to: .finalizing)
        operation.adopt(session)
        XCTAssertFalse(operation.cancel())
        XCTAssertFalse(session.cancelled)
        XCTAssertEqual(process.cancellations, 0)
        XCTAssertEqual(try operation.observation().phase, .finalizing)
        XCTAssertTrue(operation.reservesWork)
        session.finish(.done)
        XCTAssertEqual(try operation.observation().phase, .done)
        XCTAssertFalse(operation.reservesWork)
    }

    func testTerminalOperationCannotStartLateSession() throws {
        for cancelled in [false, true] {
            let operation = NativeInstallOperation(operationID: UUID(),
                expectedSavedConfigurationDigest: String(repeating: "a", count: 64))
            if cancelled { XCTAssertTrue(operation.cancel()) } else { operation.fail("synthetic refusal") }
            let process = NativeInstallOwnedAdoptionProcess()
            let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
                validate: { _ in XCTFail("Terminal operation must not validate"); return nil },
                schedule: { _ in XCTFail("Terminal operation must not schedule") },
                execution: .init(process: process))
            XCTAssertFalse(operation.start(session))
            XCTAssertEqual(session.stage, .idle)
            XCTAssertEqual(process.resets, 0)
            XCTAssertEqual(try operation.observation().phase, cancelled ? .cancelled : .failed)
        }
    }
}
