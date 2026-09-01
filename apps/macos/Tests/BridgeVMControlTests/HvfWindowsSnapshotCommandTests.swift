import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotCommandTests: XCTestCase {
    func testPlanUsesOnlyManagedPairAndBundledHelper() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let plan = try HvfWindowsSnapshotCommand.plan(
            config: fixture.config, repoRoot: fixture.repo, operation: .create)
        XCTAssertEqual(plan.snapshot.path,
                       fixture.bundle.appendingPathComponent("metadata/snapshots/latest.snapshot").path)
        XCTAssertEqual(plan.vmID, "test-vm")
        XCTAssertEqual(plan.quotaBytes, 68)
        XCTAssertEqual(plan.arguments(for: .create).first, "create")
        XCTAssertEqual(plan.arguments(for: .restore).first, "restore")
    }

    func testRestoreRequiresExistingSnapshotDirectory() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        XCTAssertThrowsError(try HvfWindowsSnapshotCommand.plan(
            config: fixture.config, repoRoot: fixture.repo, operation: .restore))
        let snapshot = fixture.bundle.appendingPathComponent("metadata/snapshots/latest.snapshot")
        try FileManager.default.createDirectory(at: snapshot, withIntermediateDirectories: true)
        XCTAssertNoThrow(try HvfWindowsSnapshotCommand.plan(
            config: fixture.config, repoRoot: fixture.repo, operation: .restore))
    }

    func testPlanRejectsMediaOutsideBundleAndSymlinkedHelper() throws {
        var fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        fixture.config.uefiVarsPath = fixture.root.appendingPathComponent("other-vars.fd").path
        try Data(count: 4).write(to: URL(fileURLWithPath: fixture.config.uefiVarsPath))
        XCTAssertThrowsError(try HvfWindowsSnapshotCommand.plan(
            config: fixture.config, repoRoot: fixture.repo, operation: .create))

        fixture = try makeFixture(in: fixture.root.appendingPathComponent("second"))
        let helper = fixture.repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        try FileManager.default.removeItem(at: helper)
        try FileManager.default.createSymbolicLink(atPath: helper.path, withDestinationPath: "/bin/true")
        XCTAssertThrowsError(try HvfWindowsSnapshotCommand.plan(
            config: fixture.config, repoRoot: fixture.repo, operation: .create))
    }

    private struct Fixture {
        let root: URL
        let repo: URL
        let bundle: URL
        var config: HvfEngineConfig
    }

    private func makeFixture(in suppliedRoot: URL? = nil) throws -> Fixture {
        let root = suppliedRoot ?? FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-snapshot-command-\(UUID())", isDirectory: true)
        let repo = root.appendingPathComponent("repo", isDirectory: true)
        let bundle = root.appendingPathComponent("test-vm/bundle.vmbridge", isDirectory: true)
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        try FileManager.default.createDirectory(at: disk.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: vars.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(count: 64).write(to: disk)
        try Data(count: 4).write(to: vars)
        XCTAssertTrue(FileManager.default.createFile(atPath: helper.path, contents: Data("#!/bin/sh\n".utf8)))
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
        let config = HvfEngineConfig(
            targetDiskPath: disk.path, uefiVarsPath: vars.path,
            evidenceDir: bundle.appendingPathComponent("logs/hvf").path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4,
            clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: bundle.appendingPathComponent("metadata/hvf.ctl").path,
            vtpmStateDir: bundle.appendingPathComponent("metadata/vtpm").path,
            vtpmKeyID: "test-vm", allowsExperimental3D: false)
        return Fixture(root: root, repo: repo, bundle: bundle, config: config)
    }
}
