import XCTest
@testable import BridgeVMControl

final class NativeCreatePayloadGuidanceTests: XCTestCase {
    @MainActor
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
        let model = LibraryModel(rootURL: library, migrateLegacy: false, startsModelsAutomatically: false)
        let view = HvfWindowsInstallView(config: config, library: model, session: .init(plan: plan))
        XCTAssertEqual(view.installerForNewVM?.path, request.isoPath)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(view.installerForNewVM)), try Data(contentsOf: iso))

        let paths = HvfWindowsInstallFinalization.paths(slug: config.slug, libraryRoot: library,
                                                        bundlePath: config.bundlePath)
        try fm.createDirectory(at: paths.transaction, withIntermediateDirectories: false)
        XCTAssertNil(view.installerForNewVM, "An interrupted install can recover without the original driver payload.")
        XCTAssertFalse(result.text.contains("Next: bridgevm app install"))
        XCTAssertTrue(result.text.contains("can’t install Windows"))
        XCTAssertTrue(result.text.contains("create it again with --fetch-drivers"))
        // Its install fails on the missing drivers; the next step names the way out, not a retry.
        let failed = NativeInstallObservation(operationID: "op", phase: .failed, acceptedUptime: 0,
            workerPending: false, sessionRunning: false, canCancel: false, logTail: [],
            failure: HvfWindowsInstallPlan.missingDriverFiles)
        XCTAssertFalse(failed.cliNextAction(vmID: config.slug).contains("retry installation"))
        XCTAssertTrue(failed.cliNextAction(vmID: config.slug).contains("--fetch-drivers"))
        XCTAssertTrue(NativeCLI.createWindowsHelp.contains("Without --fetch-drivers the VM has no driver files"))
        XCTAssertFalse(NativeCLI.createWindowsHelp.contains("bridgevm app install ID"))
    }
}
