import XCTest
@testable import BridgeVMControl

final class VMCreationSafetyTests: XCTestCase {
    func testVMNameNormalizationTrimsAndRejectsUnsafeNames() {
        XCTAssertEqual(VMLibrary.normalizedVMName("  정상 VM  \n"), "정상 VM")
        XCTAssertNil(VMLibrary.normalizedVMName(" \n\t "))
        XCTAssertNil(VMLibrary.normalizedVMName("line\nbreak"))
        XCTAssertNil(VMLibrary.normalizedVMName("control\u{0000}character"))
        XCTAssertNil(VMLibrary.normalizedVMName(String(repeating: "a", count: VMLibrary.maximumVMNameCharacters + 1)))
        XCTAssertNil(VMLibrary.normalizedVMName(String(repeating: "한", count: 100)))
    }

    func testPathContainmentUsesComponentsInsteadOfStringPrefixes() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("vm", isDirectory: true)
        let sibling = temp.appendingPathComponent("vm-backups", isDirectory: true)

        XCTAssertTrue(VMLibrary.isSameOrDescendant(source.appendingPathComponent("child"), of: source))
        XCTAssertTrue(VMLibrary.isSameOrDescendant(source, of: source))
        XCTAssertFalse(VMLibrary.isSameOrDescendant(sibling, of: source))
    }

    private func makeTempDir() throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
