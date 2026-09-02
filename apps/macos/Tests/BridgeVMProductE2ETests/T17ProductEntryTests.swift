import XCTest
@testable import BridgeVMProductE2E

final class T17ProductEntryTests: XCTestCase {
    func testFreshLibraryUsesVisibleToolbarCreateEntry() {
        XCTAssertEqual(T17UIContract.initialCreateControlIdentifier,
                       "bridgevm.library.toolbar.create")
    }
}
