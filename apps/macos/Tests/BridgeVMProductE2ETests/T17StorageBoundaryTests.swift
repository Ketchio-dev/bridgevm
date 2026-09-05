import XCTest
@testable import BridgeVMProductE2E

final class T17StorageBoundaryTests: XCTestCase {
    func testLegacyInternalBoundaryRemainsAvailable() throws {
        try T17StorageBoundary.validate("/tmp/bridgevm-e2e-fixture.abcdef/lane-1")
    }
    func testExternalStorageNeverAcceptsAnUnpinnedOrAbsentVolume() {
        for path in ["/Volumes/PortableSSD/anything", "/Volumes/PortableSSD/BridgeVM/live-t17/not-a-uuid/bridgevm-e2e-fixture.abcdef/lane-1",
                     "/Volumes/absent-t17-volume/BridgeVM/live-t17/0A306B2D-D4D3-4B9C-8A9E-007657927166/bridgevm-e2e-fixture.abcdef/lane-1"] {
            XCTAssertThrowsError(try T17StorageBoundary.validate(path))
        }
    }
}
