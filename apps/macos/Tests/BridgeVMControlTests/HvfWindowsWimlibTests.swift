import XCTest
@testable import BridgeVMControl

final class HvfWindowsWimlibTests: XCTestCase {
    func testBundleOwnedHelperIsPreferredAndMustBeARegularExecutable() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-wimlib-\(UUID().uuidString)")
        let helper = root.appendingPathComponent(HvfWindowsWimlib.bundledRelativePath)
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertNotEqual(HvfWindowsWimlib.resolve(repoRoot: root), helper.path)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: helper.path, contents: Data("#!/bin/sh\nexit 0\n".utf8)))
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: helper.path)
        XCTAssertEqual(HvfWindowsWimlib.resolve(repoRoot: root), helper.path)
        XCTAssertEqual(HvfWindowsWimlib.candidates(repoRoot: root).first, helper.path)
    }

    func testSymlinkHelperIsRejected() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-wimlib-link-\(UUID().uuidString)")
        let helper = root.appendingPathComponent(HvfWindowsWimlib.bundledRelativePath)
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createSymbolicLink(atPath: helper.path, withDestinationPath: "/bin/true")
        XCTAssertNotEqual(HvfWindowsWimlib.resolve(repoRoot: root), helper.path)
    }
}
