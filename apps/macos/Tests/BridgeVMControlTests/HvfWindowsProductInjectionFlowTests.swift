import XCTest
@testable import BridgeVMControl

extension HvfWindowsKernelPolicyVerifierTests {
    func testFreshInstallPublishesOnlyVerifiedBundleSnapshot() throws {
        let fixture = try makeFixture()
        let iso = fixture.root.appendingPathComponent("windows.iso")
        try Data("iso".utf8).write(to: iso)
        let storage = fixture.root.appendingPathComponent("install-library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Verified Install", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: true, driverPackageDir: fixture.package.path,
            storageDir: storage, persist: false, verificationDate: fixture.now,
            trustAnchors: [fixture.anchor]))
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(
            request.driverPackageDir, HvfWindowsPreparedPackageSnapshot.bundleRelativePath)
        XCTAssertNil(request.importedMedia)
        let snapshot = URL(fileURLWithPath: config.bundlePath)
            .appendingPathComponent(HvfWindowsPreparedPackageSnapshot.bundleRelativePath)
        assertSuccess(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: snapshot, now: fixture.now, trustAnchors: [fixture.anchor]))

        try appendData(
            Data("source mutation".utf8),
            to: fixture.package.appendingPathComponent("viogpu3d.sys"))
        assertSuccess(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: snapshot, now: fixture.now, trustAnchors: [fixture.anchor]))
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: fixture.package, now: fixture.now,
            trustAnchors: [fixture.anchor]), .hashMismatch)
    }

    func testImportedInjectionKeepsCanonicalMediaUntouchedUntilFinalize() throws {
        let fixture = try makeFixture()
        let sourceDisk = fixture.root.appendingPathComponent("installed.raw")
        let sourceVars = fixture.root.appendingPathComponent("vars.fd")
        let iso = fixture.root.appendingPathComponent("windows.iso")
        let diskPrefix = Data("installed-prefix".utf8)
        let varsPrefix = Data("vars-prefix".utf8)
        try diskPrefix.write(to: sourceDisk); try varsPrefix.write(to: sourceVars)
        let varsHandle = try FileHandle(forWritingTo: sourceVars)
        try varsHandle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes); try varsHandle.close()
        try Data("iso".utf8).write(to: iso)
        let storage = fixture.root.appendingPathComponent("import-library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let config = try XCTUnwrap(VMLibrary.createWindowsHVF(
            name: "Verified Import", targetDiskPath: sourceDisk.path,
            varsPath: sourceVars.path, storageDir: storage, injectViogpu3d: true,
            injectionISOPath: iso.path, driverPackageDir: fixture.package.path,
            persist: false, verificationDate: fixture.now,
            trustAnchors: [fixture.anchor]))
        XCTAssertEqual(config.installPending, true)
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.importedMedia, true)
        let plan = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), bundlePath: config.bundlePath,
            slug: config.slug, request: request)
        XCTAssertNil(plan.injectionValidationError(
            now: fixture.now, trustAnchors: [fixture.anchor]))

        let canonicalDisk = URL(fileURLWithPath: try XCTUnwrap(config.diskPath))
        let canonicalVars = URL(fileURLWithPath: config.bundlePath)
            .appendingPathComponent("metadata/hvf-vars.fd")
        try HvfWindowsInjectionWorkspace.cloneImportedMedia(plan)
        defer { HvfWindowsInjectionWorkspace.cleanup(plan) }
        XCTAssertEqual(try prefix(canonicalDisk, diskPrefix.count), diskPrefix)
        XCTAssertEqual(try prefix(canonicalVars, varsPrefix.count), varsPrefix)
        try Data("temporary mutation".utf8).write(
            to: URL(fileURLWithPath: plan.tmpTargetPath), options: [.atomic])
        XCTAssertEqual(try prefix(canonicalDisk, diskPrefix.count), diskPrefix)
        XCTAssertEqual(try Data(contentsOf: sourceDisk), diskPrefix)
    }

    private func prefix(_ url: URL, _ count: Int) throws -> Data {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        return try XCTUnwrap(handle.read(upToCount: count))
    }

    private func appendData(_ data: Data, to url: URL) throws {
        let handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd(); try handle.write(contentsOf: data); try handle.close()
    }
}
