import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallCacheTests: XCTestCase {
    func testCacheRequiresMatchingDigestReceipt() async throws {
        let temporary = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-install-cache-\(UUID())", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temporary) }
        try FileManager.default.createDirectory(at: temporary, withIntermediateDirectories: true)
        let iso = temporary.appendingPathComponent("windows.iso")
        try Data("iso".utf8).write(to: iso)
        let request = HvfWindowsInstallRequest(
            isoPath: iso.path, diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        var plan = HvfWindowsInstallPlan(
            repoRoot: temporary, bundlePath: "/tmp/bundle", slug: "cache", request: request)
        plan.homeDirectory = temporary.path
        try FileManager.default.createDirectory(
            at: URL(fileURLWithPath: plan.sourceImagePath).deletingLastPathComponent(),
            withIntermediateDirectories: true)
        let image = Data("derived installer source".utf8)
        try image.write(to: URL(fileURLWithPath: plan.sourceImagePath))
        XCTAssertFalse(plan.sourceImageCacheCandidateExists)
        let digest = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(plan.sourceImagePath))
        try Data((digest + "\n").utf8).write(to: URL(fileURLWithPath: plan.sourceImageSHA256Path))
        XCTAssertTrue(plan.sourceImageCacheCandidateExists)
        let verified = await plan.sourceImageCacheIsVerified(); XCTAssertTrue(verified)
        try Data("tampered".utf8).write(to: URL(fileURLWithPath: plan.sourceImagePath))
        let tampered = await plan.sourceImageCacheIsVerified(); XCTAssertFalse(tampered)
    }
}
