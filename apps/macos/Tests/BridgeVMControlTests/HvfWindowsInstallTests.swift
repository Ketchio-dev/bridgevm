import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallTests: XCTestCase {

    // MARK: request persistence

    func testInstallRequestRoundTripsThroughBundleMetadata() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let request = HvfWindowsInstallRequest(
            isoPath: "/tmp/example.iso", diskGiB: 96,
            injectViogpu3d: true, driverPackageDir: "/tmp/pkg")
        XCTAssertTrue(request.save(bundlePath: temp.path))
        let loaded = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: temp.path))
        XCTAssertEqual(loaded, request)
    }

    // MARK: plan paths and commands

    func testPlanConfinesDestructiveMediaToBridgevmTmpNamespace() throws {
        let plan = try makePlan(slug: "win-a")
        XCTAssertTrue(plan.tmpTargetPath.hasPrefix("/tmp/bridgevm-appinstall-win-a"))
        XCTAssertTrue(plan.tmpVarsPath.hasPrefix("/tmp/bridgevm-appinstall-win-a"))
        XCTAssertTrue(plan.tmpEvidenceDir.hasPrefix("/tmp/bridgevm-appinstall-win-a"))
    }

    func testPlanSourceCacheKeyTracksIsoNameAndSize() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("Win11 Test.iso")
        try Data(count: 4096).write(to: iso)
        var request = HvfWindowsInstallRequest(
            isoPath: iso.path, diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        var plan = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), bundlePath: "/bundle",
            slug: "vm", request: request)
        plan.homeDirectory = "/Users/example"
        XCTAssertEqual(plan.sourceImagePath, "/Users/example/BridgeVM/bridgevm-app-src/win11-test-4096.raw")
        XCTAssertTrue(HvfWindowsInstallPlan.whitespaceFree(plan.sourceImagePath))

        // A different ISO size must produce a different cached source image.
        try Data(count: 8192).write(to: iso)
        request.isoPath = iso.path
        var regrown = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), bundlePath: "/bundle",
            slug: "vm", request: request)
        regrown.homeDirectory = "/Users/example"
        XCTAssertEqual(regrown.sourceImagePath, "/Users/example/BridgeVM/bridgevm-app-src/win11-test-8192.raw")
    }

    func testInstallCommandCarriesFreshTargetSizeAndRelease() throws {
        let plan = try makePlan(slug: "big", diskGiB: 128)
        let command = plan.installCommand()
        XCTAssertEqual(command.first, "bash")
        XCTAssertTrue(command.contains("scripts/run-hvf-windows-scripted-install.sh"))
        let sizeIndex = try XCTUnwrap(command.firstIndex(of: "--fresh-target-size"))
        XCTAssertEqual(command[sizeIndex + 1], String(UInt64(128) * 1024 * 1024 * 1024))
        XCTAssertTrue(command.contains("--release"))
        XCTAssertTrue(command.contains(plan.tmpTargetPath))
    }

    func testSourceAndInjectorBuildsPassEnvironmentNotShellStrings() throws {
        let plan = try makePlan(slug: "envy", inject: true)
        let source = plan.sourceBuildCommand()
        XCTAssertEqual(source.environment["ISO"], plan.request.isoPath)
        XCTAssertEqual(source.environment["OUT"], plan.sourceImagePath)
        let injector = try XCTUnwrap(plan.injectorBuildCommand())
        XCTAssertEqual(injector.environment["VIOGPU3D_DIR"], plan.request.driverPackageDir)
        XCTAssertEqual(injector.environment["OUT"], plan.injectorImagePath)
    }

    func testInjectorBuildIsOmittedWithoutInjection() throws {
        let plan = try makePlan(slug: "plain", inject: false)
        XCTAssertNil(plan.injectorBuildCommand())
    }

    // MARK: validation

    func testValidationRejectsMissingIsoAndDriverPackage() throws {
        let missingISO = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), bundlePath: "/bundle", slug: "vm",
            request: HvfWindowsInstallRequest(
                isoPath: "/nonexistent/win.iso", diskGiB: 64,
                injectViogpu3d: false, driverPackageDir: nil))
        XCTAssertNotNil(missingISO.validationError())

        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 16).write(to: iso)
        let injectWithoutPackage = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), bundlePath: "/bundle", slug: "vm",
            request: HvfWindowsInstallRequest(
                isoPath: iso.path, diskGiB: 64,
                injectViogpu3d: true, driverPackageDir: nil))
        if let message = injectWithoutPackage.validationError() {
            // Requires either the driver-package message or an earlier tooling
            // prerequisite miss (wimlib/vars template) on minimal CI hosts.
            XCTAssertFalse(message.isEmpty)
        } else {
            XCTFail("injection without a driver package must not validate")
        }
    }

    func testDriverPackageValidationDemandsInfSysAndNoWhitespace() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let spaced = temp.appendingPathComponent("has space", isDirectory: true)
        try FileManager.default.createDirectory(at: spaced, withIntermediateDirectories: true)
        XCTAssertNotNil(HvfWindowsInstallPlan.driverPackageError(spaced.path))

        let pkg = temp.appendingPathComponent("pkg", isDirectory: true)
        try FileManager.default.createDirectory(at: pkg, withIntermediateDirectories: true)
        XCTAssertNotNil(HvfWindowsInstallPlan.driverPackageError(pkg.path))
        try Data().write(to: pkg.appendingPathComponent("viogpu3d.inf"))
        XCTAssertNotNil(HvfWindowsInstallPlan.driverPackageError(pkg.path))
        try Data().write(to: pkg.appendingPathComponent("viogpu3d.sys"))
        XCTAssertNil(HvfWindowsInstallPlan.driverPackageError(pkg.path))
    }

    // MARK: create factory

    func testCreateWindowsHVFInstallPersistsPendingRequest() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 1024).write(to: iso)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Fresh Windows \(UUID().uuidString.prefix(8))",
            isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, persist: false))

        XCTAssertEqual(config.backendKind, "hvf-engine")
        XCTAssertEqual(config.installPending, true)
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.diskGiB, 64)
        XCTAssertFalse(request.injectViogpu3d)
        // Install-pending VMs must not produce a bootable engine config.
        XCTAssertNil(HvfEngineConfig.libraryVM(config))
    }

    func testCreateWindowsHVFInstallHonorsCustomResources() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 1024).write(to: iso)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Big Windows \(UUID().uuidString.prefix(8))",
            isoPath: iso.path, diskGiB: 128,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, memMiB: 16384, cpuCount: 8, persist: false))
        XCTAssertEqual(config.memMiB, 16384)
        XCTAssertEqual(config.cpuCount, 8)
    }

    func testNetworkToggleReachesWrapperArguments() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 1024).write(to: iso)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let offline = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Offline \(UUID().uuidString.prefix(6))",
            isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, memMiB: 6144, cpuCount: 4,
            networkEnabled: false, persist: false))
        XCTAssertEqual(offline.networkEnabled, false)
        var offlineReady = offline
        offlineReady.installPending = false
        let offlineConfig = try XCTUnwrap(HvfEngineConfig.libraryVM(offlineReady))
        XCTAssertFalse(offlineConfig.virtioNet)
        XCTAssertFalse(offlineConfig.wrapperArguments().contains("--virtio-net"))

        let online = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "Online \(UUID().uuidString.prefix(6))",
            isoPath: iso.path, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, memMiB: 6144, cpuCount: 4,
            networkEnabled: true, persist: false))
        var onlineReady = online
        onlineReady.installPending = false
        let onlineConfig = try XCTUnwrap(HvfEngineConfig.libraryVM(onlineReady))
        XCTAssertTrue(onlineConfig.virtioNet)
        XCTAssertTrue(onlineConfig.wrapperArguments().contains("--virtio-net"))
    }

    func testLibraryVMDefaultsNetworkOnWhenUnset() throws {
        // Existing VMs (no networkEnabled key) must keep the NIC on.
        let config = VMConfig(
            id: "vm", name: "vm", displayName: "vm", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/vm-bundle-\(UUID().uuidString)",
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "vm", displayWidth: 1280, displayHeight: 800,
            installPending: false)
        let engine = try XCTUnwrap(HvfEngineConfig.libraryVM(config))
        XCTAssertTrue(engine.virtioNet)
    }

    func testApplyResourceOverrideRewritesLaunchSpecResources() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let metadata = temp.appendingPathComponent("metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        let launch = metadata.appendingPathComponent("apple-vz-launch.json")
        let handoff = metadata.appendingPathComponent("handoff.json")
        let seed: [String: Any] = ["resources": ["memory": "4096", "cpu": "4", "balloon_device": true]]
        let data = try JSONSerialization.data(withJSONObject: seed)
        try data.write(to: launch)
        try data.write(to: handoff)

        XCTAssertTrue(VMLibrary.applyResourceOverride(bundlePath: temp.path, memMiB: 12288, cpuCount: 6))
        for file in [launch, handoff] {
            let root = try JSONSerialization.jsonObject(with: Data(contentsOf: file)) as? [String: Any]
            let resources = try XCTUnwrap(root?["resources"] as? [String: Any])
            XCTAssertEqual(resources["memory"] as? String, "12288")
            XCTAssertEqual(resources["cpu"] as? String, "6")
            // unrelated keys are preserved
            XCTAssertEqual(resources["balloon_device"] as? Bool, true)
        }
    }

    func testCreateWindowsHVFInstallRejectsSmallDisks() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 1024).write(to: iso)
        XCTAssertNil(VMLibrary.createWindowsHVFInstall(
            name: "Too Small", isoPath: iso.path, diskGiB: 32,
            injectViogpu3d: false, driverPackageDir: nil,
            storageDir: temp, persist: false))
    }

    // MARK: injection marker → engine config

    func testPendingInjectionMarkerFeedsWrapperArguments() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let bundle = temp.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        for sub in ["disks", "metadata"] {
            try FileManager.default.createDirectory(
                at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
        }
        let injector = bundle.appendingPathComponent("disks/viogpu3d-injector.raw")
        try Data(count: 512).write(to: injector)
        let marker = bundle.appendingPathComponent(HvfWindowsInstallPlan.injectPendingMarker)
        try Data("\(injector.path)\n".utf8).write(to: marker)

        let injection = try XCTUnwrap(HvfEngineConfig.pendingInjection(bundlePath: bundle.path))
        XCTAssertEqual(injection.injectorPath, injector.path)

        var config = try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "vm", name: "vm", displayName: "vm", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "vm", displayWidth: 1280, displayHeight: 800,
            installPending: false)))
        XCTAssertEqual(config.placeholderNsid1Path, injector.path)
        XCTAssertTrue(config.bootTimerDesktopAgent)
        let arguments = config.wrapperArguments()
        let placeholderIndex = try XCTUnwrap(arguments.firstIndex(of: "--placeholder-nsid1"))
        XCTAssertEqual(arguments[placeholderIndex + 1], injector.path)
        XCTAssertTrue(arguments.contains("--boot-timer-desktop-agent"))

        // Without the marker the same VM boots with no injector attached.
        try FileManager.default.removeItem(at: marker)
        config = try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "vm", name: "vm", displayName: "vm", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "vm", displayWidth: 1280, displayHeight: 800,
            installPending: false)))
        XCTAssertNil(config.placeholderNsid1Path)
        XCTAssertFalse(config.wrapperArguments().contains("--placeholder-nsid1"))
    }

    func testPendingInjectionIgnoresMarkerWithMissingInjectorImage() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let bundle = temp.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(
            at: bundle.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        let marker = bundle.appendingPathComponent(HvfWindowsInstallPlan.injectPendingMarker)
        try Data("/nonexistent/injector.raw\n".utf8).write(to: marker)
        XCTAssertNil(HvfEngineConfig.pendingInjection(bundlePath: bundle.path))
    }

    // MARK: progress filtering

    func testProgressLineFilterKeepsLoadBearingBootLines() {
        XCTAssertTrue(HvfWindowsInstallSession.isProgressLine(
            "BOOT_TIMER ramfb source=virtio-gpu state=captured elapsed_ms=1000"))
        XCTAssertTrue(HvfWindowsInstallSession.isProgressLine("BVAGENT READY host=X"))
        XCTAssertTrue(HvfWindowsInstallSession.isProgressLine(
            "NVMe disk written back: /tmp/x.raw"))
        XCTAssertFalse(HvfWindowsInstallSession.isProgressLine(
            "hv_vm_create(ipa=40) = 0x0"))
    }

    // MARK: helpers

    private func makePlan(slug: String, diskGiB: Int = 64, inject: Bool = false) throws -> HvfWindowsInstallPlan {
        let temp = try makeTempDir()
        addTeardownBlock { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("win.iso")
        try Data(count: 2048).write(to: iso)
        var driverDir: String?
        if inject {
            let pkg = temp.appendingPathComponent("pkg", isDirectory: true)
            try FileManager.default.createDirectory(at: pkg, withIntermediateDirectories: true)
            try Data().write(to: pkg.appendingPathComponent("viogpu3d.inf"))
            try Data().write(to: pkg.appendingPathComponent("viogpu3d.sys"))
            driverDir = pkg.path
        }
        return HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"),
            bundlePath: temp.appendingPathComponent("bundle").path,
            slug: slug,
            request: HvfWindowsInstallRequest(
                isoPath: iso.path, diskGiB: diskGiB,
                injectViogpu3d: inject, driverPackageDir: driverDir))
    }

    private func makeTempDir() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("hvf-install-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}
