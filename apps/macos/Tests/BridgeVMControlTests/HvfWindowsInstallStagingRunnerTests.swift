import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// The real scripted-install runner, handed the exact installer argv, against
/// prepared bundle staging whose every path contains spaces (as a library under
/// "Application Support" does). Only the hypervisor probe, codesign and the
/// disk-attaching target verifier are replaced.
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
        let tools = try installRunner(repo: plan.repoRoot, scratch: root)
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

    /// Copies the real runner beside stand-ins; returns the directory to put first on PATH.
    private func installRunner(repo: URL, scratch: URL) throws -> URL {
        let checkout = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let scripts = repo.appendingPathComponent("scripts", isDirectory: true)
        let probe = repo.appendingPathComponent("target/release/examples/hvf_gic_boot_probe")
        let tools = scratch.appendingPathComponent("fake tools", isDirectory: true)
        for directory in [scripts, probe.deletingLastPathComponent(), tools] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try FileManager.default.copyItem(
            at: checkout.appendingPathComponent("scripts/run-hvf-windows-scripted-install.sh"),
            to: scripts.appendingPathComponent("run-hvf-windows-scripted-install.sh"))
        try executable(probe, """
            for name in BRIDGEVM_NVME_DISK BRIDGEVM_NVME_DISK2 BRIDGEVM_AARCH64_UEFI_VARS BRIDGEVM_RAMFB_DUMP_DIR; do
              printf '%s=%s\\n' "$name" "${!name}"
            done
            [[ -f "$BRIDGEVM_NVME_DISK2" && ! -L "$BRIDGEVM_NVME_DISK2" && -f "$BRIDGEVM_AARCH64_UEFI_VARS" ]]
            printf W | dd of="$BRIDGEVM_NVME_DISK2" bs=1 count=1 conv=notrunc 2>/dev/null
            printf P3 > "$BRIDGEVM_RAMFB_DUMP_DIR/frame 1.ppm"
            """)
        try executable(scripts.appendingPathComponent("verify-hvf-windows-install-target.sh"), """
            [[ $# == 2 && "$1" == --target && -f "$2" && ! -L "$2" ]] || exit 1
            printf 'verified %s\\n' "$2"
            """)
        try executable(tools.appendingPathComponent("codesign"), "echo '<key>com.apple.security.hypervisor</key>'")
        return tools
    }

    private func executable(_ url: URL, _ body: String) throws {
        try Data(("#!/bin/bash\nset -euo pipefail\n" + body + "\n").utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }
}
