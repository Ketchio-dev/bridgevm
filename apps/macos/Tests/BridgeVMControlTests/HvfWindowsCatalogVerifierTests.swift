import XCTest
@testable import BridgeVMControl

final class HvfWindowsCatalogVerifierTests: XCTestCase {
    func testResolverRequiresARegularExecutableAndSourceBuildPinsIt() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-catalog-resolver-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let helper = root.appendingPathComponent("helpers/bridgevm-catalog-verify")
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("helper-v1".utf8).write(to: helper)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: helper.path)

        XCTAssertEqual(HvfWindowsCatalogVerifier.resolve(repoRoot: root), helper.path)
        let request = HvfWindowsInstallRequest(
            isoPath: root.appendingPathComponent("windows.iso").path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        let plan = HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: root,
            bundlePath: root.appendingPathComponent("bundle.vmbridge").path,
            slug: "catalog", request: request)
        XCTAssertEqual(
            plan.sourceBuildCommand().environment["WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER"],
            helper.path)

        let firstKey = plan.sourceCacheKey
        try Data("helper-v2".utf8).write(to: helper)
        let changed = HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: root,
            bundlePath: root.appendingPathComponent("bundle.vmbridge").path,
            slug: "catalog", request: request)
        XCTAssertNotEqual(changed.sourceCacheKey, firstKey)
    }

    func testResolverRejectsSymlink() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-catalog-symlink-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("target")
        let helper = root.appendingPathComponent("helpers/bridgevm-catalog-verify")
        try FileManager.default.createDirectory(
            at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("helper".utf8).write(to: target)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755], ofItemAtPath: target.path)
        try FileManager.default.createSymbolicLink(at: helper, withDestinationURL: target)
        XCTAssertNil(HvfWindowsCatalogVerifier.resolve(repoRoot: root))
    }
}
