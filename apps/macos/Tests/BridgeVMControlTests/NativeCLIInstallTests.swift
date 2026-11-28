import XCTest
@testable import BridgeVMControl

final class NativeCLIInstallTests: XCTestCase {
    func testInstallCommandsAcceptOnlyOneCanonicalID() throws {
        XCTAssertEqual(try NativeCLIOptions.parse(arguments: ["install", "개발-vm"]).command,
                       .install("개발-vm"))
        XCTAssertEqual(try NativeCLIOptions.parse(arguments: ["install-status", "개발-vm"]).command,
                       .installStatus("개발-vm"))
        XCTAssertEqual(try NativeCLIOptions.parse(arguments: ["install-cancel", "개발-vm"]).command,
                       .installCancel("개발-vm"))
        for verb in ["install", "install-status", "install-cancel"] {
            XCTAssertTrue(try NativeCLIOptions.parse(arguments: [verb, "--help"]).showHelp)
            for arguments in [[verb], [verb, "../vm"], [verb, "vm", "extra"],
                              [verb, "vm", "--iso", "/secret.iso"]] {
                XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments))
            }
        }
    }

    func testAbsentLibraryIsNotCreatedAndReturnsUnavailable() {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("install-cli-absent-" + UUID().uuidString)
        let result = NativeCLIInstall.run(rootURL: root, id: "vm", operation: .install)
        XCTAssertFalse(result.complete)
        XCTAssertEqual(result.unavailableReason, "ownerUnavailable")
        XCTAssertNotNil(result.requestedOperationID)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testTextShowsThePlainStepAndDoesNotClaimCompletion() {
        let observation = NativeInstallObservation(operationID: UUID().uuidString,
            phase: .installing, acceptedUptime: 1, workerPending: false,
            sessionRunning: true, canCancel: true, logTail: ["bounded line"], failure: nil)
        let result = NativeCLIInstallResult(command: "installStatus", vmID: "vm",
            libraryPath: "/library", appInstanceID: UUID().uuidString,
            requestedOperationID: nil, disposition: .status, observation: observation,
            unavailableReason: nil, complete: true)
        XCTAssertTrue(result.text.contains("Status: Installing Windows"))
        XCTAssertTrue(result.text.contains("Recent log:\nbounded line"))
        XCTAssertFalse(result.text.contains("Complete"))
        XCTAssertFalse(result.text.contains("guest boot confirmed"))
    }
}
