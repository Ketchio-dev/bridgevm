import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOwnedAdoptionTests: XCTestCase {
    func testNormalStartRetainsOneSessionAndSchedulesOnce() async throws {
        let fixture = NativeInstallOwnedAdoptionFixture()
        defer { fixture.clean() }
        try fixture.store.save(fixture.store.config())
        let library = fixture.library()
        let install = try fixture.request(.install, operationID: UUID())
        let accepted = try library.requestOwnedInstall(install, validateOwner: {})
        await fixture.waitForAcknowledgment()
        let session = try XCTUnwrap(fixture.session)
        let repeated = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertTrue(repeated.operation === accepted.operation)
        XCTAssertEqual(repeated.disposition, .existing)
        XCTAssertEqual(try accepted.operation.observation().phase, .preparingSource)
        XCTAssertTrue(accepted.operation.reservesWork)
        XCTAssertTrue(session.isRunning)
        XCTAssertEqual(fixture.made, 1)
        XCTAssertEqual(fixture.process.resets, 1)
        XCTAssertEqual(fixture.inspection.snapshot.calls, 1)
        XCTAssertEqual(fixture.validator.snapshot.calls, 1)
        XCTAssertEqual(fixture.queue.jobs.count, 1)
        XCTAssertEqual(fixture.process.requests, 0)
    }

    func testSynchronousValidatingCancellationOwnsSessionUntilAcknowledgedAndSchedulesNothing() async throws {
        let fixture = NativeInstallOwnedAdoptionFixture()
        defer { fixture.clean() }
        try fixture.store.save(fixture.store.config())
        let library = fixture.library()
        let savedPath = fixture.store.root.appendingPathComponent("windows/vm.json")
        let saved = try Data(contentsOf: savedPath)
        let cancel = try fixture.request(.installCancel)
        var callbackCount = 0
        fixture.onValidating = { session in
            callbackCount += 1
            do {
                let result = try library.requestOwnedInstall(cancel, validateOwner: {})
                XCTAssertEqual(result.disposition, .cancelAccepted)
                XCTAssertTrue(session.cancelled, "Cancellation must reach the starting session synchronously")
                XCTAssertTrue(result.operation.reservesWork, "Cancellation must retain ownership until acknowledgment")
                XCTAssertFalse(result.operation.cancelledBeforeSession)
                XCTAssertEqual(try result.operation.observation().phase, .cancelling)
            } catch { XCTFail("Validating-stage cancellation was refused: \(error)") }
        }
        let accepted = try library.requestOwnedInstall(fixture.request(.install, operationID: UUID()), validateOwner: {})
        await fixture.waitForAcknowledgment()
        let session = try XCTUnwrap(fixture.session)
        XCTAssertEqual(callbackCount, 1)
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertFalse(session.isRunning)
        XCTAssertEqual(try accepted.operation.observation().phase, .cancelled)
        XCTAssertFalse(accepted.operation.reservesWork)
        XCTAssertFalse(accepted.operation.cancelledBeforeSession)
        XCTAssertEqual(fixture.process.resets, 1)
        XCTAssertEqual(fixture.process.cancellations, 1)
        XCTAssertEqual(fixture.inspection.snapshot.calls, 0)
        XCTAssertEqual(fixture.validator.snapshot.calls, 0)
        XCTAssertTrue(fixture.queue.jobs.isEmpty, "Cancellation must not schedule any install pipeline")
        XCTAssertEqual(fixture.sourceChecks, 0)
        XCTAssertEqual(fixture.mediaPreparations, 0)
        XCTAssertEqual(fixture.finalizations, 0)
        XCTAssertEqual(fixture.process.requests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.plan.stagingDirectory))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.plan.sourceImagePath))
        XCTAssertEqual(try Data(contentsOf: savedPath), saved)
        XCTAssertThrowsError(try library.requestOwnedInstall(cancel, validateOwner: {})) {
            XCTAssertEqual($0 as? NativeInstallControlRefusal, .cancellationUnavailable)
        }
    }

    func testStartRefusalFailsOperationAndReleasesReservation() async throws {
        let fixture = NativeInstallOwnedAdoptionFixture()
        defer { fixture.clean() }
        try fixture.store.save(fixture.store.config())
        let library = fixture.library(refuseStart: true)
        let accepted = try library.requestOwnedInstall(fixture.request(.install, operationID: UUID()), validateOwner: {})
        await fixture.waitForAcknowledgment()
        XCTAssertEqual(try accepted.operation.observation().phase, .failed)
        XCTAssertNotNil(try accepted.operation.observation().failure)
        XCTAssertFalse(accepted.operation.reservesWork)
        XCTAssertEqual(fixture.process.resets, 0)
        XCTAssertTrue(fixture.queue.jobs.isEmpty)
    }
}
