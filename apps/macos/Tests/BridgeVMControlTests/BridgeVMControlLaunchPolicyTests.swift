import XCTest
@testable import BridgeVMControl

final class BridgeVMControlLaunchPolicyTests: XCTestCase {
    func testLegacyImportIsSkippedForAnExplicitE2ELibraryRoot() throws {
        let e2e = try XCTUnwrap(URL(string: "file:///tmp/bridgevm-e2e-test/library"))
        let explicit = BridgeVMControlLaunchOptions(e2eLibraryRoot: e2e, e2eUnattendedPath: nil)
        XCTAssertFalse(BridgeVMControlLaunchPolicy.shouldMigrateLegacy(options: explicit))
    }

    func testLegacyImportStaysOnForTheDefaultLibrary() {
        XCTAssertTrue(BridgeVMControlLaunchPolicy.shouldMigrateLegacy(options: nil))
        let plain = BridgeVMControlLaunchOptions(e2eLibraryRoot: nil, e2eUnattendedPath: nil)
        XCTAssertTrue(BridgeVMControlLaunchPolicy.shouldMigrateLegacy(options: plain))
    }
}
