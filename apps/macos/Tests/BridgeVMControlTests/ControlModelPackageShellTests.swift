import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelPackageShellTests: XCTestCase {
    func testMissingRequiredBashRefusesBeforeInvokingSudo() throws {
        let fixture = try PackageCommandFixture(status: 0)
        try FileManager.default.removeItem(at: fixture.bin.appendingPathComponent("bash"))
        let result = fixture.run(try XCTUnwrap(ControlModel.packageInstallCommand(["git"])))
        XCTAssertEqual(result.code, 127)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.argumentsFile.path))
        try fixture.assertUnchanged()
    }
}
