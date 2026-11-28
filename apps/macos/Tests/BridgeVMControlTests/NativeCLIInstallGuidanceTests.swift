import XCTest
@testable import BridgeVMControl

final class NativeCLIInstallGuidanceTests: XCTestCase {
    func testActiveTerminalAndDonePhasesGiveDistinctNextActions() {
        let active = result(.installing), failed = result(.failed, failure: "synthetic")
        let cancelled = result(.cancelled), done = result(.done)
        XCTAssertTrue(active.text.contains("To follow progress, run `bridgevm app install-status 개발-vm`."))
        XCTAssertTrue(failed.text.contains("To try again, run `bridgevm app install 개발-vm`."))
        XCTAssertTrue(cancelled.text.contains("To try again, run `bridgevm app install 개발-vm`."))
        XCTAssertTrue(done.text.contains("Next: run `bridgevm app readiness 개발-vm`"))
        XCTAssertFalse(active.text.contains("Request: completed"))
    }

    /// Status and install text use plain step words; operation IDs, phase codes and the scope stay in JSON.
    func testTextUsesPlainStepsAndKeepsIDsInJSON() throws {
        let active = result(.installing), failed = result(.failed, failure: "Windows installation couldn’t start.")
        XCTAssertEqual(active.text, """
        Windows installation: 개발-vm
        Status: Installing Windows
        To follow progress, run `bridgevm app install-status 개발-vm`.
        To cancel, run `bridgevm app install-cancel 개발-vm`.

        """)
        XCTAssertTrue(failed.text.contains("Status: Failed\nWindows installation couldn’t start.\n"), failed.text)
        let operationID = try XCTUnwrap(active.observation?.operationID)
        for value in [active, failed] {
            XCTAssertFalse(value.text.contains(operationID), value.text)
            XCTAssertFalse(value.text.contains("app-owned-install"), value.text)
        }
        let json = String(decoding: try JSONEncoder().encode(active), as: UTF8.self)
        XCTAssertTrue(json.contains(operationID) && json.contains("\"installing\""), json)
        let accepted = NativeCLIInstallResult(command: "install", vmID: "vm", libraryPath: "/library",
            appInstanceID: nil, requestedOperationID: UUID().uuidString, disposition: .accepted,
            observation: nil, unavailableReason: nil, complete: true)
        XCTAssertEqual(accepted.text, "Windows installation: vm\nInstallation started.\n")
    }

    func testTextGuidanceDoesNotChangeVersionedJSON() throws {
        let value = result(.done)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        XCTAssertEqual(object["schema"] as? String, "bridgevm.app-install-command.v2")
        XCTAssertEqual(object["scope"] as? String, "app-owned-install")
        XCTAssertEqual(object["complete"] as? Bool, true)
        XCTAssertEqual((object["observation"] as? [String: Any])?["phase"] as? String, "done")
        XCTAssertNil(object["nextAction"])
    }

    private func result(_ phase: NativeInstallObservation.Phase,
                        failure: String? = nil) -> NativeCLIInstallResult {
        let observation = NativeInstallObservation(operationID: UUID().uuidString,
            phase: phase, acceptedUptime: 1,
            workerPending: phase == .preparingPlan, sessionRunning: phase == .installing,
            canCancel: phase == .installing, logTail: [], failure: failure)
        return NativeCLIInstallResult(command: "installStatus", vmID: "개발-vm",
            libraryPath: "/library", appInstanceID: UUID().uuidString,
            requestedOperationID: nil, disposition: .status, observation: observation,
            unavailableReason: nil, complete: true)
    }
}
