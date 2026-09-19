import XCTest
@testable import BridgeVMProductE2E

final class T17SupportedAttributeTests: XCTestCase {
    private struct ReadFailure: Error {}

    func testUnadvertisedAttributeDoesNotReadItsValue() throws {
        var reads = 0
        let result: String? = try T17SupportedAttribute.read(
            "AXIdentifier", advertised: { ["AXRole"] }
        ) {
            reads += 1
            throw ReadFailure()
        }
        XCTAssertNil(result)
        XCTAssertEqual(reads, 0)
    }

    func testAdvertisedAttributeReadsItsValue() throws {
        let result = T17SupportedAttribute.read(
            "AXIdentifier", advertised: { ["AXRole", "AXIdentifier"] }
        ) { "bridgevm.windows.install.view" }
        XCTAssertEqual(result, "bridgevm.windows.install.view")
    }

    func testAdvertisedAttributeReadFailurePropagates() {
        XCTAssertThrowsError(try readWithValueFailure()) { error in
            XCTAssertTrue(error is ReadFailure)
        }
    }

    func testAttributeListFailurePropagatesWithoutReadingValue() {
        var reads = 0
        XCTAssertThrowsError(try T17SupportedAttribute.read(
            "AXIdentifier", advertised: { throw ReadFailure() }
        ) { reads += 1; return "unreachable" }) { error in
            XCTAssertTrue(error is ReadFailure)
        }
        XCTAssertEqual(reads, 0)
    }

    private func readWithValueFailure() throws -> String? {
        try T17SupportedAttribute.read(
            "AXIdentifier", advertised: { ["AXIdentifier"] }
        ) { throw ReadFailure() }
    }
}
