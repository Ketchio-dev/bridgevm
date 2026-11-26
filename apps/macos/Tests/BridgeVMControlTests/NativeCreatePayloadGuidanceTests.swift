import XCTest
@testable import BridgeVMControl

final class NativeCreatePayloadGuidanceTests: XCTestCase {
    func testActualCLIRegistrationDoesNotSuggestUnfulfillableInstallStep() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("cli-create-payload-" + UUID().uuidString)
        try fm.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: root) }
        let iso = root.appendingPathComponent("synthetic.iso")
        try Data("synthetic iso".utf8).write(to: iso)
        let library = root.appendingPathComponent("library")
        let options = NativeCLICreateWindowsOptions(name: "Pending", isoPath: iso.path,
            diskGiB: 64, memoryMiB: 6144, cpuCount: 2,
            resolution: .init(width: 1280, height: 800), networkEnabled: false)
        let result = try NativeCLICreateWindows.create(options, libraryRoot: library)
        let config = try NativeLibraryReader.readConfig(rootURL: library, id: result.id)
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertNil(request.guestPayloadDirectory)
        XCTAssertNil(request.guestPayloadManifest)
        let repository = root.appendingPathComponent("synthetic-resources")
        // Only readability is needed to reach the existing payload guard. Nothing executes.
        for relative in HvfWindowsInstallPlan.installResourcePaths {
            let path = repository.appendingPathComponent(relative)
            try fm.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("synthetic resource".utf8).write(to: path)
        }
        let plan = HvfWindowsInstallPlan(repoRoot: repository, libraryRoot: library,
            bundlePath: config.bundlePath, slug: config.slug, request: request)
        XCTAssertEqual(plan.validationError(), "Windows Setup needs the driver files stored with this VM.")
        XCTAssertFalse(result.text.contains("Next: bridgevm app install"))
        XCTAssertTrue(result.text.contains("required signed ARM64 guest driver payload"))
        XCTAssertTrue(result.text.contains("Existing CLI entries cannot be completed"))
        XCTAssertTrue(NativeCLI.createWindowsHelp.contains(NativeCLICreateWindowsGuidance.limitation))
        XCTAssertFalse(NativeCLI.createWindowsHelp.contains("bridgevm app install ID"))
    }
}
