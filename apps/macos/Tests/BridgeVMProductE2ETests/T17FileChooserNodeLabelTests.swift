import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserNodeLabelTests: XCTestCase {
    func testReadFailureNamesTheNodeAndKeepsCodeAndDetail() {
        XCTAssertThrowsError(try T17FileChooserNodeLabel.naming(7, describe: { "AXGroup/-/n\($0)" }) { () -> Int in
            throw T17FileChooser.failure("file chooser AXChildren read failed; ax_error=-25200")
        }) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "input-selection-failed",
                detail: "file chooser AXChildren read failed; ax_error=-25200; node=AXGroup/-/n7"))
        }
    }
    func testSuccessfulReadIsUnchangedAndUndescribed() throws {
        var described = false
        XCTAssertEqual(try T17FileChooserNodeLabel.naming(1, describe: { _ in described = true; return "" }) { 42 }, 42)
        XCTAssertFalse(described)
    }
    func testOtherErrorsPassThroughUnnamed() {
        struct Other: Error {}
        XCTAssertThrowsError(try T17FileChooserNodeLabel.naming(1, describe: { _ in "x" }) { () -> Int in throw Other() }) {
            XCTAssertTrue($0 is Other)
        }
    }
    func testLabelKeepsOnlyRoleSubroleAndIdentifierCharacters() {
        XCTAssertEqual(T17FileChooserNodeLabel.label(role: "AXOutline", subrole: nil, identifier: "sidebar"), "AXOutline/-/sidebar")
        XCTAssertEqual(T17FileChooserNodeLabel.label(role: nil, subrole: "AXSearchField", identifier: "my file;/Vol"), "?/AXSearchField/myfileVol")
        XCTAssertEqual(T17FileChooserNodeLabel.sanitized("윈도우 ISO"), "ISO")
        XCTAssertEqual(T17FileChooserNodeLabel.sanitized(String(repeating: "a", count: 90)).count, 40)
    }
}
