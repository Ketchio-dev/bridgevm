import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserNodeLabelTests: XCTestCase {
    private func named(_ code: AXError) -> T17Blocker? {
        do { _ = try T17FileChooserNodeLabel.naming(7, describe: { "AXGroup/-/n\($0)" }) { () -> Int in
            throw T17FileChooser.failure("file chooser AXChildren read failed; ax_error=\(code.rawValue)") }
        } catch { return error as? T17Blocker }
        return nil
    }
    func testNamedReadFailureKeepsCodeDetailAndTransientClassification() throws {
        XCTAssertEqual(named(.failure), T17Blocker(code: "input-selection-failed",
            detail: "node=AXGroup/-/n7; file chooser AXChildren read failed; ax_error=-25200"))
        for code in [AXError.failure, .invalidUIElement, .cannotComplete] {  // r62 lost retries when the label was a suffix
            XCTAssertTrue(T17FileChooserSnapshot.isTransientReadFailure(try XCTUnwrap(named(code))))
        }
    }
    func testSuccessIsUndescribedAndOtherErrorsPassThrough() throws {
        struct Other: Error {}
        XCTAssertEqual(try T17FileChooserNodeLabel.naming(1, describe: { _ in XCTFail(); return "" }) { 42 }, 42)
        XCTAssertThrowsError(try T17FileChooserNodeLabel.naming(1, describe: { _ in "x" }) { () -> Int in throw Other() }) { XCTAssertTrue($0 is Other) }
    }
    func testLabelKeepsOnlyRoleSubroleAndIdentifierCharacters() {
        XCTAssertEqual(T17FileChooserNodeLabel.label(role: "AXOutline", subrole: nil, identifier: "sidebar"), "AXOutline/-/sidebar")
        XCTAssertEqual(T17FileChooserNodeLabel.label(role: nil, subrole: "AXSearchField", identifier: "my file;/Vol"), "?/AXSearchField/myfileVol")
        XCTAssertEqual(T17FileChooserNodeLabel.sanitized("윈도우 ISO"), "ISO")
        XCTAssertEqual(T17FileChooserNodeLabel.sanitized(String(repeating: "a", count: 90)).count, 40)
    }
}
