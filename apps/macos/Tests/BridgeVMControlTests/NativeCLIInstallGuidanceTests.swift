import XCTest
@testable import BridgeVMControl

final class NativeCLIInstallGuidanceTests: XCTestCase {
    private let digest = String(repeating: "a", count: 64)

    func testActiveTerminalAndDonePhasesGiveDistinctNextActions() {
        let active = result(.installing), failed = result(.failed, failure: "synthetic")
        let cancelled = result(.cancelled), done = result(.done)
        XCTAssertTrue(active.text.contains("inspect installation status"))
        XCTAssertTrue(failed.text.contains("explicitly retry installation"))
        XCTAssertTrue(cancelled.text.contains("explicitly retry installation"))
        XCTAssertTrue(done.text.contains("check readiness"))
        XCTAssertFalse(active.text.contains("Request: completed"))
        XCTAssertTrue(active.text.contains("Control exchange: complete"))
    }

    func testTextGuidanceDoesNotChangeVersionedJSON() throws {
        let value = result(.done)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        XCTAssertEqual(object["schema"] as? String, "bridgevm.app-install-command.v1")
        XCTAssertEqual(object["scope"] as? String, "app-owned-install")
        XCTAssertEqual(object["complete"] as? Bool, true)
        XCTAssertEqual((object["observation"] as? [String: Any])?["phase"] as? String, "done")
        XCTAssertNil(object["nextAction"])
    }

    private func result(_ phase: NativeInstallObservation.Phase,
                        failure: String? = nil) -> NativeCLIInstallResult {
        let observation = NativeInstallObservation(operationID: UUID().uuidString,
            expectedSavedConfigurationDigest: digest, phase: phase, acceptedUptime: 1,
            workerPending: phase == .preparingPlan, sessionRunning: phase == .installing,
            canCancel: phase == .installing, logTail: [], failure: failure)
        return NativeCLIInstallResult(command: "installStatus", vmID: "개발-vm",
            libraryPath: "/library", appInstanceID: UUID().uuidString,
            requestedOperationID: nil, disposition: .status, observation: observation,
            unavailableReason: nil, complete: true)
    }
}
