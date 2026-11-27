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

    /// A driver folder opened in Finder gets a .DS_Store; it is not a driver file, so the VM's identity is
    /// the same with or without it (the stage script leaves it out of the exact file set too).
    func testFinderDSStoreIsNotADriverFile() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-payload-finder-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let payload = root.appendingPathComponent("payload/storage", isDirectory: true)
        try FileManager.default.createDirectory(at: payload, withIntermediateDirectories: true)
        try Data("driver".utf8).write(to: payload.appendingPathComponent("driver.sys"))
        let manifest = root.appendingPathComponent("manifest.tsv")
        try Data("manifest".utf8).write(to: manifest)
        let folder = root.appendingPathComponent("payload").path
        let plain = HvfWindowsGuestPayloadIdentity.inspect(payloadDirectory: folder, manifestPath: manifest.path)
        try Data("Bud1".utf8).write(to: root.appendingPathComponent("payload/.DS_Store"))
        try Data("Bud1".utf8).write(to: payload.appendingPathComponent(".DS_Store"))

        XCTAssertEqual(try HvfWindowsGuestPayloadTree.scan(URL(fileURLWithPath: folder)).map(\.relativePath),
                       ["storage/driver.sys"])
        let withFinder = HvfWindowsGuestPayloadIdentity.inspect(payloadDirectory: folder, manifestPath: manifest.path)
        XCTAssertNil(withFinder.error)
        XCTAssertEqual(withFinder.digest, plain.digest)
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
