import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOperationAdmissionTests: XCTestCase {
    func testAdmissionCancellationStopsBeforeSessionEffects() throws {
        for retry in [false, true] {
            let operation = NativeInstallOperation(operationID: UUID(),
                expectedSavedConfigurationDigest: String(repeating: "a", count: 64))
            let process = NativeInstallOwnedAdoptionProcess()
            let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
                validate: { _ in XCTFail("Cancelled admission must not validate"); return nil },
                schedule: { _ in XCTFail("Cancelled admission must not schedule") },
                execution: .init(process: process))
            if retry { session.finish(.failed("previous attempt")) }
            session.workAdmission = { _ in
                do {
                    XCTAssertEqual(try operation.observation().phase, .preparingPlan)
                    XCTAssertTrue(try operation.observation().canCancel)
                    XCTAssertTrue(operation.cancel())
                    XCTAssertEqual(try operation.observation().phase, .cancelled)
                } catch { XCTFail("Admission cancellation failed: \(error)") }
                return nil
            }
            defer { session.workAdmission = nil }
            XCTAssertFalse(operation.start(session))
            XCTAssertEqual(try operation.observation().phase, .cancelled)
            XCTAssertFalse(operation.reservesWork)
            XCTAssertFalse(session.isRunning)
            XCTAssertEqual(process.resets, 0)
            XCTAssertEqual(process.requests, 0)
        }
    }

    func testAdmissionCallbackRetainsReservationAndRefusalAllowsTerminalFailure() throws {
        for retry in [false, true] {
            let store = NativeInstallOperationStore(), digest = String(repeating: "a", count: 64)
            guard case let .accepted(operation) = store.reserve(vmID: "windows", digest: digest,
                operationID: UUID()) else { return XCTFail("Expected reservation") }
            let process = NativeInstallOwnedAdoptionProcess()
            let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan(),
                validate: { _ in XCTFail("Refused attempt must not validate"); return nil },
                schedule: { _ in XCTFail("Refused attempt must not schedule") },
                execution: .init(process: process))
            if retry { session.finish(.failed("previous synthetic attempt")) }
            var callbacks = 0
            session.workAdmission = { _ in
                callbacks += 1
                XCTAssertFalse(session.isRunning)
                XCTAssertTrue(operation.reservesWork)
                do {
                    let observation = try operation.observation()
                    XCTAssertEqual(observation.phase, .preparingPlan)
                    XCTAssertTrue(observation.canCancel)
                } catch { XCTFail("Admission observation must remain valid: \(error)") }
                guard case let .existing(repeated) = store.reserve(vmID: "windows", digest: digest,
                    operationID: UUID()) else { XCTFail("Admission must retain the reservation"); return "refused" }
                XCTAssertTrue(repeated === operation)
                XCTAssertFalse(operation.start(session), "Reentrant operation start must be refused")
                return "synthetic admission refusal"
            }
            defer { session.workAdmission = nil }
            XCTAssertFalse(operation.start(session))
            XCTAssertEqual(callbacks, 1)
            operation.fail("synthetic start refusal")
            XCTAssertEqual(try operation.observation().phase, .failed)
            XCTAssertFalse(operation.reservesWork)
            XCTAssertEqual(process.resets, 0)
            guard case .accepted = store.reserve(vmID: "windows", digest: digest,
                operationID: UUID()) else { return XCTFail("A refused start must permit a new reservation") }
        }
    }
}
