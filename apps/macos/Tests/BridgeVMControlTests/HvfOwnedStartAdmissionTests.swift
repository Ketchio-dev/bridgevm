import Foundation
import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
enum HvfOwnedStartTestSupport {
    static let digest = String(repeating: "a", count: 64)
    static func admit(_ session: HvfEngineSession, id: UUID = UUID(),
                      registered: @MainActor (HvfOwnedStartOperation) throws -> Void = { _ in },
                      revalidate: @escaping @MainActor () -> Bool = { true }) throws -> HvfOwnedStartOperation {
        guard case let .accepted(ticket) = session.requestOwnedStart(configuration: session.config, operationID: id,
            expectedSavedConfigurationDigest: digest, onReserved: registered, revalidate: revalidate) else {
            throw CocoaError(.executableRuntimeMismatch)
        }
        return ticket
    }
    static func wait(_ predicate: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while !predicate(), ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertTrue(predicate(), "Bounded asynchronous fixture did not finish")
    }
}

@MainActor
final class HvfOwnedStartAdmissionTests: XCTestCase {
    func testPrivateReservationAndOuterRegistrationPrecedePublicationAndWorker() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var registered = false, notifications = 0, nestedRefusals = 0
        session.ownedStartWorker = { _ in .failure(.readiness) }
        let observer = session.objectWillChange.sink {
            notifications += 1
            XCTAssertTrue(registered)
            if session.hasPendingOwnedStart {
                XCTAssertNil(session.reserveRuntimeMutation(kind: .snapshot))
                if case .refused = session.start() { nestedRefusals += 1 }
            }
        }
        defer { observer.cancel() }
        let ticket = try HvfOwnedStartTestSupport.admit(session, registered: { ticket in
            XCTAssertTrue(session.matchesRuntimeReservation(ticket.operationID))
            XCTAssertEqual(notifications, 0)
            XCTAssertNil(session.reserveRuntimeMutation(kind: .vtpmReset))
            registered = true
        })
        XCTAssertTrue(notifications > 0); XCTAssertTrue(nestedRefusals > 0)
        try await HvfOwnedStartTestSupport.wait { !ticket.workerPending }
        XCTAssertEqual(f.launches, 0); XCTAssertEqual(f.lookups, 0)
    }

    func testRegistrationFailureReleasesOnlyNoEffectsReservation() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var notifications = 0
        let observer = session.objectWillChange.sink { notifications += 1 }; defer { observer.cancel() }
        session.ownedStartWorker = { _ in XCTFail("Rejected registration cannot schedule a worker"); return .failure(.readiness) }
        let result = session.requestOwnedStart(configuration: f.config, operationID: UUID(),
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest,
            onReserved: { _ in throw CocoaError(.fileWriteNoPermission) }, revalidate: { true })
        guard case .refused(.admissionRefused) = result else { return XCTFail("Expected refusal") }
        XCTAssertEqual(notifications, 0); XCTAssertFalse(session.hasActiveRuntimeWork)
        XCTAssertNil(session.ownedStartOperation); XCTAssertEqual(f.launches, 0)
    }

    func testRetryReturnsIdenticalTicketAndChangedConfigurationCannotReuseID() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); session.ownedStartWorker = { _ in .failure(.readiness) }
        let ticket = try HvfOwnedStartTestSupport.admit(session)
        var callbacks = 0
        let retry = session.requestOwnedStart(configuration: f.config, operationID: ticket.operationID,
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest,
            onReserved: { XCTAssertTrue($0 === ticket); callbacks += 1 }, revalidate: { false })
        guard case let .existing(same) = retry else { return XCTFail("Expected same retained operation") }
        XCTAssertTrue(same === ticket); XCTAssertEqual(callbacks, 1)
        var different = f.config; different.ramMiB += 128
        let conflict = session.requestOwnedStart(configuration: different, operationID: ticket.operationID,
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, onReserved: { _ in XCTFail("Conflict") }, revalidate: { true })
        guard case .refused(.operationConflict) = conflict else { return XCTFail("ID rebinding must fail") }
        try await HvfOwnedStartTestSupport.wait { !ticket.workerPending }
    }

    func testReservedAdmissionDoesNotBypassUnrelatedWorkOrEditedConfig() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(); var checks = 0
        session.reservedWorkAdmission = { id, report in
            checks += 1; XCTAssertTrue(session.matchesRuntimeReservation(id)); XCTAssertFalse(report)
            XCTAssertNil(session.reserveRuntimeMutation(kind: .snapshot))
            return "Another retained session owns this VM"
        }
        let result = session.requestOwnedStart(configuration: f.config, operationID: UUID(),
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, onReserved: { _ in }, revalidate: { true })
        guard case .refused(.admissionRefused) = result else { return XCTFail("Unrelated work must refuse") }
        XCTAssertEqual(checks, 1); XCTAssertFalse(session.hasPendingOwnedStart)
        var changed = f.config; changed.ramMiB += 128
        let mismatch = session.requestOwnedStart(configuration: changed, operationID: UUID(),
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, onReserved: { _ in XCTFail("Config mismatch") }, revalidate: { true })
        guard case .refused(.configurationMismatch) = mismatch else { return XCTFail("Exact config required") }
        XCTAssertEqual(f.launches, 0)
    }
}
