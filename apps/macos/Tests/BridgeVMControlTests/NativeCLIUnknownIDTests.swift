import XCTest
@testable import BridgeVMControl

/// readiness and the snapshot commands answered a mistyped ID with "Cannot open the VM directory
/// safely.", which reads like a security problem.
final class NativeCLIUnknownIDTests: XCTestCase {
    func testAMistypedIDNamesTheIDAndHowToFindIt() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertThrowsError(try NativeLibraryReader.readConfig(rootURL: root, id: "dev-mv")) { error in
            XCTAssertEqual(error.localizedDescription,
                           "No VM has the ID “dev-mv” in this library. Run `bridgevm app list` to find its ID.")
        }
    }
}
