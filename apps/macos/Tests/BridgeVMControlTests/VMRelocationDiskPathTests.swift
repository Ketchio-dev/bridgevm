import Darwin
import XCTest
@testable import BridgeVMControl

final class VMRelocationDiskPathTests: XCTestCase {
    func testVarAndPrivateVarSpellingsRebaseTheSameExistingDirectory() throws {
        try check(source: "/var", selected: "/private/var/tmp", suffix: "tmp")
        try check(source: "/private/var", selected: "/var/tmp", suffix: "tmp")
    }
    func testTmpAndPrivateTmpSpellingsRebaseTheSameExistingDirectory() throws {
        try check(source: "/tmp", selected: "/private/tmp", suffix: nil)
        try check(source: "/private/tmp", selected: "/tmp", suffix: nil)
    }
    private func check(source: String, selected: String, suffix: String?) throws {
        // Read existing system aliases only. No system file or directory is moved,
        // written, or created; the prospective destination stays on owned scratch.
        let source = URL(fileURLWithPath: source)
        let equivalent = suffix.map { source.appendingPathComponent($0) } ?? source
        var lhs = stat(), rhs = stat()
        XCTAssertEqual(stat(equivalent.path, &lhs), 0)
        XCTAssertEqual(stat(selected, &rhs), 0)
        XCTAssertEqual(lhs.st_dev, rhs.st_dev)
        XCTAssertEqual(lhs.st_ino, rhs.st_ino)
        let destination = LibraryDeletionTestDirectory.temporary.appendingPathComponent("unused-move-" + UUID().uuidString)
        let expected = suffix.map { destination.appendingPathComponent($0) } ?? destination
        XCTAssertEqual(VMRelocationDiskPath.rebased(selected, source: source, destination: destination), expected.path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
    }
}
