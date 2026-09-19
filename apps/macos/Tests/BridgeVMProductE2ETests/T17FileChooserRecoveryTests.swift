import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserRecoveryTests: XCTestCase {
    func testInvalidElementRetriesArePacedWithoutChangingTheAttemptCeiling() throws {
        var roots = 0, pauses = 0
        let value: Int = try T17FileChooserSnapshot.read(pause: { pauses += 1 }, root: {
            roots += 1; return roots
        }, nodes: { (root: Int) -> [Int] in [root] }, project: { (nodes: [Int]) -> Int in
            if nodes[0] < 3 { throw T17FileChooser.failure("file chooser AXIdentifier read failed; ax_error=\(AXError.invalidUIElement.rawValue)") }
            return nodes[0]
        })
        XCTAssertEqual(value, 3); XCTAssertEqual(roots, 3); XCTAssertEqual(pauses, 2)
    }

    func testNonRetryableFailureDoesNotPause() {
        var pauses = 0
        XCTAssertThrowsError(try T17FileChooserSnapshot.read(pause: { pauses += 1 }, root: { 1 }, nodes: { [$0] }, project: { _ -> Int in
            throw T17FileChooser.failure("file chooser AXIdentifier read failed; ax_error=-25201")
        }))
        XCTAssertEqual(pauses, 0)
    }

    func testStageRetainsFailureCodeAndAttributesTheOperation() {
        XCTAssertThrowsError(try T17FileChooserStage.run("panel-appearance") { throw T17FileChooser.failure("stale") } as Void) { error in
            XCTAssertEqual((error as? T17Blocker)?.code, "input-selection-failed")
            XCTAssertEqual((error as? T17Blocker)?.detail, "stage=panel-appearance; stale")
        }
    }
}
