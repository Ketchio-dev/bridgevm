import XCTest
@testable import BridgeVMControl
final class HvfWindowsAtomicMediaPublisherTests: XCTestCase {
    func testExistingCanonicalMediaPairIsReplaced() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = (root.appendingPathComponent("target.raw"), root.appendingPathComponent("new.raw"))
        let vars = (root.appendingPathComponent("vars.fd"), root.appendingPathComponent("new-vars.fd"))
        try Data("old".utf8).write(to: disk.0); try Data("new".utf8).write(to: disk.1)
        try Data("old-vars".utf8).write(to: vars.0); try Data("new-vars".utf8).write(to: vars.1)
        try HvfWindowsMediaPublicationTransaction.publish(
            disk: (disk.0.path, disk.1.path), vars: (vars.0.path, vars.1.path))
        XCTAssertEqual(try Data(contentsOf: disk.0), Data("new".utf8))
        XCTAssertEqual(try Data(contentsOf: vars.0), Data("new-vars".utf8))
        XCTAssertFalse(FileManager.default.fileExists(atPath: disk.1.path))
        XCTAssertFalse(try FileManager.default.contentsOfDirectory(atPath: root.path)
            .contains(where: { $0.contains(".staging-") }))
    }
    func testSecondStagingFailureLeavesCanonicalPairUnchanged() throws {
        let root = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("target.raw")
        let vars = root.appendingPathComponent("vars.fd")
        let source = root.appendingPathComponent("new.raw")
        try Data("disk".utf8).write(to: disk); try Data("vars".utf8).write(to: vars)
        try Data("new".utf8).write(to: source)
        XCTAssertThrowsError(try HvfWindowsMediaPublicationTransaction.publish(
            disk: (disk.path, source.path),
            vars: (vars.path, root.appendingPathComponent("missing.fd").path)))
        XCTAssertEqual(try Data(contentsOf: disk), Data("disk".utf8))
        XCTAssertEqual(try Data(contentsOf: vars), Data("vars".utf8))
    }
    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: false)
        return url
    }
}
