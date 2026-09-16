import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOperationTests: XCTestCase {
    private let operationID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    private let digest = String(repeating: "a", count: 64)

    private func operation() -> NativeInstallOperation {
        .init(operationID: operationID, expectedSavedConfigurationDigest: digest, acceptedUptime: 10)
    }

    private func session() -> HvfWindowsInstallSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let request = HvfWindowsInstallRequest(isoPath: "/missing.iso", isoSHA256: digest,
                                               diskGiB: 64, injectViogpu3d: false,
                                               driverPackageDir: nil)
        let plan = HvfWindowsInstallPlan(repoRoot: root, libraryRoot: root,
                                         bundlePath: root.path, slug: "windows", request: request)
        return HvfWindowsInstallSession(plan: plan)
    }

    func testPreparationIsRetainedAndCancellationBecomesTerminal() throws {
        let value = operation()
        XCTAssertTrue(value.reservesWork)
        XCTAssertEqual(try value.observation().phase, .preparingPlan)
        XCTAssertTrue(value.cancel())
        XCTAssertFalse(value.reservesWork)
        XCTAssertTrue(value.cancelledBeforeSession)
        XCTAssertEqual(try value.observation().phase, .cancelled)
        XCTAssertFalse(value.cancel())
    }

    func testFailureIsBoundedAndCannotBeReplacedByLateSession() throws {
        let value = operation(), late = session()
        value.fail(String(repeating: "실패", count: 3_000))
        value.adopt(late)
        let observed = try value.observation()
        XCTAssertEqual(observed.phase, .failed)
        XCTAssertTrue(try XCTUnwrap(observed.failure).utf8.count <=
            NativeInstallControlCodec.maximumFailureBytes)
        XCTAssertFalse(value.reservesWork)
    }

    func testAdoptedSessionMapsStagesAndBoundsNewestLogs() throws {
        let value = operation(), session = session()
        session.attemptActive = true
        session.transition(to: .installing)
        for index in 0..<80 { session.appendLog("\(index)-" + String(repeating: "x", count: 5_000)) }
        value.adopt(session)
        let active = try value.observation()
        XCTAssertEqual(active.phase, .installing)
        XCTAssertTrue(active.sessionRunning)
        XCTAssertEqual(active.logTail.count, 8)
        XCTAssertTrue(active.logTail.last?.hasPrefix("79-") == true)
        XCTAssertTrue(active.logTail.reduce(0) { $0 + $1.utf8.count } <=
            NativeInstallControlCodec.maximumLogBytes)
        session.finish(.done)
        XCTAssertEqual(try value.observation().phase, .done)
        XCTAssertFalse(value.reservesWork)
    }

    func testAdoptedCancellationUsesTheExistingSession() throws {
        let value = operation(), session = session()
        session.attemptActive = true
        session.transition(to: .validating)
        value.adopt(session)
        XCTAssertTrue(value.cancel())
        XCTAssertEqual(session.stage, .cancelling)
        let observed = try value.observation()
        XCTAssertEqual(observed.phase, .cancelling)
        XCTAssertFalse(observed.canCancel)
    }
}
