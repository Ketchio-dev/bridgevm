import XCTest
@testable import BridgeVMControl

final class WindowsHVFProductPolicyTests: XCTestCase {
    func testInstallDoesNotRequireTemplateAndOwnsISO() throws {
        for mode in [CreateVMSheet.Mode.ubuntu, .iso, .windows] {
            XCTAssertTrue(WindowsHVFProductPolicy.requiresTemplate(mode))
        }
        for mode in [CreateVMSheet.Mode.windowsHVF, .windowsHVFInstall] {
            XCTAssertFalse(WindowsHVFProductPolicy.requiresTemplate(mode))
        }

        let temp = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("selected.iso")
        let contents = Data(repeating: 0xa5, count: 1024)
        try contents.write(to: iso)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        let config = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "E2E \(UUID().uuidString.prefix(8))", isoPath: iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, persist: false))
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.isoPath, config.bundlePath + "/disks/installer.iso")
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: request.isoPath)), contents)
        XCTAssertEqual(request.isoSHA256, HvfWindowsInstallCacheIdentity.sha256File(request.isoPath))
        try FileManager.default.removeItem(at: iso)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: request.isoPath)), contents)
        XCTAssertEqual(config.experimental3DAllowed, false)
    }

    func testReadyInstallAndImportStayThreeDOff() throws {
        let temp = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("selected.iso")
        try Data(count: 1024).write(to: iso)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        var installed = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(
            name: "3D Off \(UUID().uuidString.prefix(8))", isoPath: iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: storage, persist: false))
        installed.installPending = false
        try assertThreeDOff(try XCTUnwrap(HvfEngineConfig.libraryVM(installed)))

        let disk = temp.appendingPathComponent("installed.raw")
        let vars = temp.appendingPathComponent("vars.fd")
        try Data([1]).write(to: disk)
        XCTAssertTrue(FileManager.default.createFile(atPath: vars.path, contents: Data()))
        let varsHandle = try FileHandle(forWritingTo: vars)
        try varsHandle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes)
        try varsHandle.close()
        let imported = try XCTUnwrap(VMLibrary.createWindowsHVF(
            name: "Imported 3D Off \(UUID().uuidString.prefix(8))",
            targetDiskPath: disk.path, varsPath: vars.path,
            storageDir: storage, persist: false))
        XCTAssertEqual(imported.experimental3DAllowed, false)
        try assertThreeDOff(try XCTUnwrap(HvfEngineConfig.libraryVM(imported)))
    }

    func testInstallUsesBundledVarsSeedWithoutTemplateArgument() throws {
        let temporary = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temporary) }
        let output = temporary.appendingPathComponent("vars.fd")
        try HvfWindowsBootSeed.writeBundledSeed(to: output.path)
        XCTAssertEqual(try Data(contentsOf: output), try HvfWindowsBootSeed.bundledSeed())

        let request = HvfWindowsInstallRequest(
            isoPath: "/private/tmp/windows.iso", diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        let plan = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/app/Contents/Resources"),
            bundlePath: "/private/tmp/vm.bundle", slug: "product-e2e", request: request)
        XCTAssertFalse(plan.installCommand().contains("--vars-template"))
    }

    func testInstallRejectsChangedManagedISOAndRecipeInvalidatesCache() throws {
        let temp = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("windows.iso")
        try Data("first".utf8).write(to: iso)
        let originalHash = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(iso.path))
        let request = HvfWindowsInstallRequest(
            isoPath: iso.path, isoSHA256: originalHash, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        let first = HvfWindowsInstallPlan(
            repoRoot: temp, bundlePath: "/tmp/bundle", slug: "sealed", request: request)
        try Data("changed".utf8).write(to: iso)
        XCTAssertEqual(first.validationError(), "선택한 Windows ISO가 VM 생성 후 변경되었습니다.")

        let script = temp.appendingPathComponent("scripts/build-hvf-windows-scripted-source.sh")
        try FileManager.default.createDirectory(at: script.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try Data("recipe-a".utf8).write(to: script)
        let currentHash = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(iso.path))
        let currentRequest = HvfWindowsInstallRequest(
            isoPath: iso.path, isoSHA256: currentHash, diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        let before = HvfWindowsInstallPlan(
            repoRoot: temp, bundlePath: "/tmp/bundle", slug: "recipe", request: currentRequest)
        try Data("recipe-b".utf8).write(to: script)
        let after = HvfWindowsInstallPlan(
            repoRoot: temp, bundlePath: "/tmp/bundle", slug: "recipe", request: currentRequest)
        XCTAssertNotEqual(before.sourceCacheKey, after.sourceCacheKey)
    }

    private func assertThreeDOff(_ config: HvfEngineConfig) throws {
        XCTAssertFalse(config.virtioGpu3d)
        XCTAssertFalse(config.allowsExperimental3D)
        let runner = config.runnerArguments(
            manifestPath: "/tmp/launch.json", runnerPath: "/tmp/runner",
            firmwareCodePath: "/tmp/code.fd", probePath: "/tmp/probe")
        for forbidden in ["--virtio-gpu-3d", "--virtio-gpu-device-id", "1050", "virgl"] {
            XCTAssertFalse(config.wrapperArguments().contains(forbidden))
            XCTAssertFalse(runner.contains(forbidden))
        }
    }

    private func temporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-product-policy-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}
