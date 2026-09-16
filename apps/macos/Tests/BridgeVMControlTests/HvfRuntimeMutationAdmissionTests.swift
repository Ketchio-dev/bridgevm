import Foundation
import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeMutationAdmissionTests: XCTestCase {
    func testMutationReservationPrecedesPublicationAndBlocksBothStartRoutes() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var notifications = 0
        let observer = session.objectWillChange.sink {
            notifications += 1
            guard session.hasPendingRuntimeMutation else { return }
            XCTAssertNil(session.reserveRuntimeMutation(kind: .snapshot))
            if case .refused = session.start() {} else { XCTFail("UI start overlaps mutation") }
            let result = session.requestOwnedStart(configuration: f.config, operationID: UUID(),
                expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest,
                onReserved: { _ in XCTFail("Mutation must refuse before registration") }, revalidate: { true })
            if case .refused(.mutationActive) = result {} else { XCTFail("CLI start overlaps mutation") }
        }
        defer { observer.cancel() }
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .vtpmReset))
        XCTAssertTrue(notifications > 0); XCTAssertTrue(session.hasActiveRuntimeWork)
        XCTAssertEqual(session.connectionState, .stopped, "Storage work is not a running guest")
        XCTAssertTrue(session.validateRuntimeMutation(reservation))
        session.finishRuntimeMutation(reservation)
        XCTAssertFalse(session.hasActiveRuntimeWork); XCTAssertEqual(f.launches, 0)
    }

    func testChangedConfigurationInvalidatesPendingEffectAndStaleReleaseCannotUnlockNewWork() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session()
        let first = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        session.config.ramMiB += 128
        XCTAssertFalse(session.validateRuntimeMutation(first)); XCTAssertThrowsError(try first.checkEffect())
        XCTAssertTrue(session.hasPendingRuntimeMutation, "Invalidated work stays reserved until its owner returns")
        XCTAssertNil(session.reserveRuntimeMutation(kind: .vtpmReset))
        session.finishRuntimeMutation(first)
        let second = try XCTUnwrap(session.reserveRuntimeMutation(kind: .vtpmReset))
        session.finishRuntimeMutation(first)
        XCTAssertTrue(session.matchesRuntimeReservation(second.id)); XCTAssertTrue(second.isActive)
        session.finishRuntimeMutation(second)
    }

    func testLibraryAdmissionRecheckedBeforeSnapshotFilesystemOrProcessEffects() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var stillRegistered = true, checks = 0
        session.reservedWorkAdmission = { id, _ in
            checks += 1
            XCTAssertTrue(session.matchesRuntimeReservation(id))
            return stillRegistered ? nil : "The library registration was removed"
        }
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        defer { session.finishRuntimeMutation(reservation) }
        let output = f.root.appendingPathComponent("must-not-exist/latest.snapshot")
        let plan = HvfWindowsSnapshotCommand.Plan(executable: URL(fileURLWithPath: "/usr/bin/false"),
            disk: URL(fileURLWithPath: f.config.targetDiskPath), vars: URL(fileURLWithPath: f.config.uefiVarsPath),
            snapshot: output, vmID: "synthetic", quotaBytes: 1)
        stillRegistered = false
        do {
            _ = try await HvfWindowsSnapshotCommand.run(.create, plan: plan) {
                guard await session.validateRuntimeMutation(reservation) else { throw CocoaError(.userCancelled) }
                try reservation.checkEffect()
            }
            XCTFail("Removed library admission cannot create or launch")
        } catch { XCTAssertEqual((error as NSError).code, CocoaError.userCancelled.rawValue) }
        XCTAssertEqual(checks, 2)
        XCTAssertFalse(FileManager.default.fileExists(atPath: output.deletingLastPathComponent().path))
    }

    func testAdmissionCallbackCannotReserveConflictingWork() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var checks = 0
        session.reservedWorkAdmission = { id, _ in
            checks += 1; XCTAssertTrue(session.matchesRuntimeReservation(id))
            XCTAssertNil(session.reserveRuntimeMutation(kind: .vtpmReset))
            XCTAssertFalse(session.acceptStartConfiguration(f.config))
            return nil
        }
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .vtpmRecoveryExport))
        XCTAssertEqual(checks, 1); session.finishRuntimeMutation(reservation)
    }
}
