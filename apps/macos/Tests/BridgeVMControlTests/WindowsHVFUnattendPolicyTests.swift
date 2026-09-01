import XCTest
@testable import BridgeVMControl

final class WindowsHVFUnattendPolicyTests: XCTestCase {
    func testE2EAnswerFileIsManagedAndBoundIntoTheSourceIdentity() throws {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-unattend-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("selected.iso")
        let answer = temp.appendingPathComponent("e2e-unattend.xml")
        try Data("iso".utf8).write(to: iso)
        try Data("<unattend nonce='a'/>".utf8).write(to: answer)
        let library = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)

        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "E2E Answer", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            e2eUnattendedPath: answer.path, storageDir: library,
            libraryRoot: library, persist: false))
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.unattendedPath,
                       config.bundlePath + "/metadata/windows-install-unattend.xml")
        XCTAssertEqual(request.unattendedIdentity,
                       HvfWindowsInstallCacheIdentity.sha256File(try XCTUnwrap(request.unattendedPath)))
        try FileManager.default.removeItem(at: answer)
        XCTAssertNotNil(HvfWindowsInstallCacheIdentity.sha256File(try XCTUnwrap(request.unattendedPath)))

        XCTAssertNotEqual(
            HvfWindowsInstallCacheIdentity.key(
                isoSHA256: "iso", unattendedIdentity: "answer-a", repoRoot: temp),
            HvfWindowsInstallCacheIdentity.key(
                isoSHA256: "iso", unattendedIdentity: "answer-b", repoRoot: temp))
        let plan = HvfWindowsInstallPlan(
            repoRoot: temp, libraryRoot: library,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        XCTAssertEqual(plan.sourceBuildCommand().environment["WINDOWS_UNATTEND_PATH"],
                       request.unattendedPath)
    }
}
