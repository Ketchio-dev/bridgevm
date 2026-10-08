import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallDiskSizeStagingTests: XCTestCase {
    func testUnrepresentableFileOffsetRefusesBeforeRemovingExistingStaging() throws {
        let f = try HvfWindowsInstallStagingFixture(diskGiB: Int(Int64.max / (1 << 30)) + 1)
        defer { f.remove() }
        let original = try f.writeInstallerOutputs(log: Data("retained evidence".utf8))
        let disk = try Data(contentsOf: original.target)
        let vars = try Data(contentsOf: original.vars)
        XCTAssertThrowsError(try HvfWindowsInstallExecution().prepareMedia(f.plan))
        XCTAssertEqual(try Data(contentsOf: original.target), disk)
        XCTAssertEqual(try Data(contentsOf: original.vars), vars)
        XCTAssertEqual(try Data(contentsOf: original.evidence.appendingPathComponent("run.log")),
                       Data("retained evidence".utf8))
    }

    func testNegativeZeroAndOverflowingSizesDoNotCreateStaging() throws {
        for size in [Int.min, -1, 0, Int.max] {
            let f = try HvfWindowsInstallStagingFixture(diskGiB: size)
            defer { f.remove() }
            XCTAssertThrowsError(try HvfWindowsInstallExecution().prepareMedia(f.plan))
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.plan.stagingDirectory))
        }
    }
}
