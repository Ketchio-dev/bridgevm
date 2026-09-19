import XCTest
@testable import BridgeVMProductE2E

final class T17ProductEntryTests: XCTestCase {
    func testFreshLibraryUsesPrimaryFirstRunCreateEntry() {
        XCTAssertEqual(T17UIContract.initialCreateControlIdentifier,
                       "bridgevm.first-run.create")
    }
}
