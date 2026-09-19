import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsGuestPayloadTreeTests: XCTestCase {
    func testIdentitySurvivesUsersToPrivateVarStaging() throws {
        let sourceRoot = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".bridgevm-payload-test-\(UUID().uuidString)")
        let destinationRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-payload-test-\(UUID().uuidString)")
        defer {
            try? FileManager.default.removeItem(at: sourceRoot)
            try? FileManager.default.removeItem(at: destinationRoot)
        }
        let source = sourceRoot.appendingPathComponent("payload/storage", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        try Data("driver".utf8).write(to: source.appendingPathComponent("driver.sys"))
        let manifest = sourceRoot.appendingPathComponent("manifest.tsv")
        try Data("manifest".utf8).write(to: manifest)
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge")
        try FileManager.default.createDirectory(
            at: bundle.appendingPathComponent("metadata"), withIntermediateDirectories: true)

        let before = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: sourceRoot.appendingPathComponent("payload").path,
            manifestPath: manifest.path)
        let staged = try XCTUnwrap(WindowsHVFGuestPayloadPolicy.stage(
            payloadDirectory: sourceRoot.appendingPathComponent("payload").path,
            manifestPath: manifest.path, in: bundle.path))

        XCTAssertNil(before.error)
        XCTAssertEqual(staged.identity, before.digest)
        XCTAssertFalse(staged.identity.isEmpty)
    }

    func testScannerRejectsSymlinkWithoutFollowingIt() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-payload-link-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let target = root.appendingPathComponent("driver.sys")
        try Data("driver".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(
            at: root.appendingPathComponent("alias.sys"), withDestinationURL: target)
        XCTAssertThrowsError(try HvfWindowsGuestPayloadTree.scan(root))
    }
}
