import XCTest
@testable import BridgeVMControl

final class WindowsHVFUnattendPolicyTests: XCTestCase {
    /// This checkout (above apps/macos/Tests/BridgeVMControlTests/<file>), for
    /// the real answer-file template.
    private let repoRoot = (0..<5).reduce(URL(fileURLWithPath: #filePath)) { url, _ in
        url.deletingLastPathComponent()
    }

    private func makeTemp() throws -> URL {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-unattend-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(
            at: temp.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        return temp
    }

    func testMacTimeZoneMapsToItsWindowsIDAndUnknownZonesToUTC() {
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "Asia/Seoul"),
                       "Korea Standard Time")
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "America/New_York"),
                       "Eastern Standard Time")
        // Not a territory 001 zone, but Eastern all the same.
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "America/Toronto"),
                       "Eastern Standard Time")
        // macOS reports IANA names; CLDR keys India by its old name, Asia/Calcutta.
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "Asia/Kolkata"),
                       "India Standard Time")
        // A zone every Windows 11 image knows, not 2026's British Columbia Standard Time.
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "America/Vancouver"),
                       "Pacific Standard Time")
        XCTAssertEqual(WindowsHVFUnattendPolicy.windowsTimeZone(for: "Mars/Olympus_Mons"), "UTC")
    }

    func testGeneratedAnswerFileIsTheTemplateWithTheMacTimeZone() throws {
        let bundle = try makeTemp()
        defer { try? FileManager.default.removeItem(at: bundle) }
        let seoul = try XCTUnwrap(TimeZone(identifier: "Asia/Seoul"))

        let staged = try XCTUnwrap(WindowsHVFUnattendPolicy.stageHostTimeZone(
            in: bundle.path, timeZone: seoul, repoRoot: repoRoot))

        XCTAssertEqual(staged.path, bundle.path + "/metadata/windows-install-unattend.xml")
        XCTAssertEqual(staged.sha256, HvfWindowsInstallCacheIdentity.sha256File(staged.path))
        let answer = try String(contentsOfFile: staged.path, encoding: .utf8)
        let template = try String(
            contentsOf: repoRoot.appendingPathComponent("scripts/win-assets/unattend.xml"),
            encoding: .utf8)
        // Both passes (specialize, oobeSystem) get the zone; nothing else changes.
        XCTAssertEqual(answer.components(
            separatedBy: "<TimeZone>Korea Standard Time</TimeZone>").count, 3)
        XCTAssertFalse(answer.contains("<TimeZone>UTC</TimeZone>"))
        XCTAssertEqual(answer.replacingOccurrences(of: "Korea Standard Time", with: "UTC"),
                       template)
        XCTAssertEqual(answer.components(separatedBy: "\r\n").count,
                       answer.components(separatedBy: "\n").count, "CRLF kept")

        XCTAssertNil(WindowsHVFUnattendPolicy.stageHostTimeZone(
            in: bundle.path, timeZone: seoul, repoRoot: bundle))
    }

    func testCreatedInstallUsesTheGeneratedAnswerFile() throws {
        let temp = try makeTemp()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("selected.iso")
        try Data("iso".utf8).write(to: iso)
        let library = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)

        guard case .created(let config)? = VMLibrary.createWindowsHVFInstall(
            name: "Local Clock", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil, storageDir: library,
            libraryRoot: library, repoRoot: repoRoot, persist: false
        ) else { return XCTFail("expected created") }

        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        let answer = config.bundlePath + "/metadata/windows-install-unattend.xml"
        XCTAssertEqual(request.unattendedPath, answer)
        XCTAssertEqual(request.unattendedIdentity, HvfWindowsInstallCacheIdentity.sha256File(answer))
        let zone = WindowsHVFUnattendPolicy.windowsTimeZone(for: TimeZone.current.identifier)
        XCTAssertTrue(try String(contentsOfFile: answer, encoding: .utf8)
            .contains("<TimeZone>\(zone)</TimeZone>"))
        let plan = HvfWindowsInstallPlan(
            repoRoot: repoRoot, libraryRoot: library,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        XCTAssertEqual(plan.sourceBuildCommand().environment["WINDOWS_UNATTEND_PATH"], answer)
        XCTAssertEqual(plan.sealedUnattendedIdentity, request.unattendedIdentity)
    }

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

        guard case .created(let config)? = VMLibrary.createWindowsHVFInstall(
            name: "E2E Answer", isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            e2eUnattendedPath: answer.path, storageDir: library,
            libraryRoot: library, persist: false) else { return XCTFail("expected created") }
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
