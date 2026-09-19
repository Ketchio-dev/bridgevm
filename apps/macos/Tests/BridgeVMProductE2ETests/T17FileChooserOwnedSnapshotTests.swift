import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserOwnedSnapshotTests: XCTestCase {
    func testRetriesReuseTheVerifiedOwner() throws {
        let owner = 41
        var visited: [Int] = []
        var attempts = 0
        let result: Int = try T17FileChooserOwnedSnapshot.read(owner: owner, nodes: { root in
            visited.append(root)
            attempts += 1
            if attempts < 3 {
                throw T17FileChooser.failure("file chooser AXIdentifier read failed; ax_error=\(AXError.invalidUIElement.rawValue)")
            }
            return [root]
        }, project: { $0[0] })
        XCTAssertEqual(result, owner)
        XCTAssertEqual(visited, [owner, owner, owner])
    }
}
