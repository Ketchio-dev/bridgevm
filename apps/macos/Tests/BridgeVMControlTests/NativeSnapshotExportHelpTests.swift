import XCTest
@testable import BridgeVMControl

final class NativeSnapshotExportHelpTests: XCTestCase {
    func testHelpWithCompleteExportArgumentsNeverSelectsExecution() throws {
        for flag in ["--help", "-h"] {
            for arguments in [["snapshot-export", "vm", "/export/snapshot", flag],
                              [flag, "snapshot-export", "vm", "/export/snapshot"],
                              ["snapshot-export", "vm", flag, "/export/snapshot", "--json"]] {
                XCTAssertTrue(try NativeCLIOptions.parse(arguments: arguments).showHelp)
            }
        }
    }

    func testHelpWithPartialExportArgumentsDoesNotRequireARegistrationOrOutput() throws {
        for arguments in [["snapshot-export", "--help"], ["snapshot-export", "vm", "--help"],
                          ["snapshot-export", "not canonical", "relative", "--help"]] {
            XCTAssertTrue(try NativeCLIOptions.parse(arguments: arguments).showHelp)
        }
        XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: ["snapshot-export", "vm", "/out", "--unknown", "--help"]))
        XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: ["snapshot-export", "vm", "/out", "--library", "relative", "--help"]))
        XCTAssertFalse(try NativeCLIOptions.parse(arguments: ["snapshot-export", "vm", "/out"]).showHelp)
    }
}
