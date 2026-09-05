import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsExternalInstallPathsTests: XCTestCase {
    func testExternalScratchStaysWithItsLibrary() {
        let lane = "/Volumes/PortableSSD/BridgeVM/live-t17/0A306B2D-D4D3-4B9C-8A9E-007657927166/bridgevm-e2e-fixture.abcdef/lane-1"
        let prefix = HvfWindowsInstallTemporaryPaths.prefix(libraryRoot: URL(fileURLWithPath: lane + "/library"), slug: "fixture")
        XCTAssertEqual(prefix, lane + "/bridgevm-appinstall-fixture")
    }
    func testInternalScratchPathDoesNotChange() {
        XCTAssertEqual(HvfWindowsInstallTemporaryPaths.prefix(libraryRoot: URL(fileURLWithPath: "/tmp/library"), slug: "fixture"), "/tmp/bridgevm-appinstall-fixture")
    }
    func testUnpinnedExternalLibraryIsRefused() {
        XCTAssertNotNil(HvfWindowsInstallTemporaryPaths.validationError(libraryRoot: URL(fileURLWithPath: "/Volumes/PortableSSD/library")))
    }
}
