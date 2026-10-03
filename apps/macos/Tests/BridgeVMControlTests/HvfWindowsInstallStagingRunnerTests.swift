import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// The real scripted-install runner, handed the exact installer argv, against
/// prepared bundle staging whose every path contains spaces (as a library under
/// "Application Support" does). The probe, codesign, process inventory and target
/// verifier are replaced; copied firmware policy authenticates synthetic bytes.
@MainActor
final class HvfWindowsInstallStagingRunnerTests: XCTestCase {
    func testRunnerUsesTheAppCreatedMediaAtStagingPathsWithSpaces() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("install runner " + UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = root.appendingPathComponent("Application Support/BridgeVM", isDirectory: true)
        let slug = "runner-" + UUID().uuidString.lowercased()
        let bundle = library.appendingPathComponent("\(slug)/Windows 11.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(
            at: bundle.appendingPathComponent("metadata"), withIntermediateDirectories: true)
        let plan = HvfWindowsInstallPlan(
            repoRoot: root.appendingPathComponent("Bridge VM.app/Contents/Resources", isDirectory: true),
            libraryRoot: library, bundlePath: bundle.path, slug: slug,
            request: HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("w.iso").path,
                                              diskGiB: 1, injectViogpu3d: false, driverPackageDir: nil))
        let tools = try HvfWindowsInstallStagingRunnerFixture.installRunner(repo: plan.repoRoot, scratch: root)
        let source = URL(fileURLWithPath: plan.sourceImagePath)
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("synthetic source".utf8).write(to: source)
        try HvfWindowsInstallExecution().prepareMedia(plan)
        let before = try XCTUnwrap(HvfWindowsInstallStagingFixture.status(URL(fileURLWithPath: plan.stagingTargetPath)))

        let output = root.appendingPathComponent("runner output.txt")
        XCTAssertTrue(FileManager.default.createFile(atPath: output.path, contents: nil))
        let handle = try FileHandle(forWritingTo: output)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = plan.installCommand()
        process.currentDirectoryURL = plan.repoRoot
        process.environment = ["PATH": tools.path + ":/usr/bin:/bin:/usr/sbin:/sbin"]
        process.standardOutput = handle
        process.standardError = handle
        try process.run()
        process.waitUntilExit()
        try handle.close()

        let transcript = (try? String(contentsOf: output, encoding: .utf8)) ?? ""
        XCTAssertEqual(process.terminationStatus, 0, transcript)
        let evidence = URL(fileURLWithPath: plan.stagingEvidenceDir, isDirectory: true)
        let log = try String(contentsOf: evidence.appendingPathComponent("run.log"), encoding: .utf8)
        XCTAssertEqual(log.split(separator: "\n").map(String.init), [
            "BRIDGEVM_NVME_DISK=\(plan.sourceImagePath)",
            "BRIDGEVM_NVME_DISK2=\(plan.stagingTargetPath)",
            try HvfWindowsInstallStagingRunnerFixture.firmwareLogLine(log, runtime: plan.repoRoot),
            "BRIDGEVM_AARCH64_UEFI_VARS=\(plan.stagingVarsPath)",
            "BRIDGEVM_RAMFB_DUMP_DIR=\(plan.stagingEvidenceDir)/ramfb",
        ])
        let after = try XCTUnwrap(HvfWindowsInstallStagingFixture.status(URL(fileURLWithPath: plan.stagingTargetPath)))
        XCTAssertEqual(after.st_ino, before.st_ino, "the runner must use, not recreate, the app-created target")
        XCTAssertEqual(UInt64(after.st_size), plan.freshTargetSizeBytes)
        XCTAssertEqual(after.st_mode & 0o7777, 0o600)
        let written = try FileHandle(forReadingFrom: URL(fileURLWithPath: plan.stagingTargetPath))
        XCTAssertEqual(try written.read(upToCount: 1), Data("W".utf8))
        try written.close()
        for name in ["preflight.txt", "target-stat.txt", "cleanup.txt", "install-success.txt", "ramfb/frame 1.ppm"] {
            XCTAssertTrue(FileManager.default.fileExists(atPath: evidence.appendingPathComponent(name).path), name)
        }
        XCTAssertEqual(try String(contentsOf: evidence.appendingPathComponent("install-success.txt"), encoding: .utf8),
                       "verified \(plan.stagingTargetPath)\n")
    }

}
