import XCTest
@testable import BridgeVMControl

/// readiness and the snapshot commands answered a mistyped ID with "Cannot open the VM directory
/// safely.", which reads like a security problem.
final class NativeCLIUnknownIDTests: XCTestCase {
    func testAMistypedIDNamesTheIDAndHowToFindIt() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertThrowsError(try NativeLibraryReader.readConfig(rootURL: root, id: "dev-mv")) { error in
            XCTAssertEqual(error.localizedDescription,
                           "No VM has the ID “dev-mv” in this library. Run `bridgevm app list` to find its ID.")
        }
        // inspect said "No native VM configuration found for ID 'dev-mv'."
        XCTAssertThrowsError(try NativeLibraryReader.snapshot(rootURL: root, id: "dev-mv")) { error in
            XCTAssertEqual(error.localizedDescription, NativeLibraryConfigLoader.unknownID("dev-mv"))
        }
        // install answered "BridgeVM isn’t installing this VM." for a request to start one.
        let install = NativeCLIInstall.run(rootURL: root, id: "dev-mv", operation: .install)
        XCTAssertFalse(install.complete)
        XCTAssertEqual(install.text, "Windows installation: dev-mv\n" + NativeLibraryConfigLoader.unknownID("dev-mv") + "\n")
        // A display name where the ID goes.
        XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: ["start", "Dev VM"])) { error in
            guard case NativeCLIError.invalid = error else { return XCTFail("\(error) exits 2") }
            XCTAssertEqual(error.localizedDescription, "“Dev VM” isn’t a VM ID. Run `bridgevm app list` to find its ID.")
        }
    }

    /// Stopping a VM that isn't running said "Couldn’t confirm that dev-vm stopped." and pointed to
    /// `bridgevm app status` without the ID, which is a usage error.
    func testStoppingAVMThatIsNotRunningSaysSo() {
        let stop = NativeCLIRuntimeStop(vmID: "dev-vm", libraryPath: "/l", target: nil, operationID: nil,
            complete: false, observation: nil, unavailableReason: NativeRuntimeStopRefused.targetUnavailable.reason)
        XCTAssertEqual(stop.text, "dev-vm isn’t running in BridgeVM, so there was nothing to stop.\n")
    }
}
