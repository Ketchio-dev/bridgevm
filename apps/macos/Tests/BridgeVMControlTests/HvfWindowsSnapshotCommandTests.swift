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
        XCTAssertEqual(try plan.arguments(for: .create).last, "144")
        XCTAssertEqual(try plan.arguments(for: .create).first, "create")
        XCTAssertEqual(try plan.arguments(for: .restore).first, "restore")
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

    /// The card's date is when the snapshot's manifest, written last, was written.
    func testSnapshotDateIsWhenItsManifestWasWritten() throws {
        let fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        XCTAssertNil(HvfWindowsSnapshotCommand.snapshotDate(config: fixture.config))
        let snapshot = fixture.bundle.appendingPathComponent("metadata/snapshots/latest.snapshot")
        try FileManager.default.createDirectory(at: snapshot, withIntermediateDirectories: true)
        XCTAssertNil(HvfWindowsSnapshotCommand.snapshotDate(config: fixture.config), "no manifest, no snapshot")
        let manifest = snapshot.appendingPathComponent("manifest.json")
        try Data("{}\n".utf8).write(to: manifest)
        let made = Date(timeIntervalSince1970: 1_794_000_000)
        try FileManager.default.setAttributes([.modificationDate: made], ofItemAtPath: manifest.path)
        XCTAssertEqual(HvfWindowsSnapshotCommand.snapshotDate(config: fixture.config), made)
    }

    /// Each refusal before the helper runs keeps its English for the CLI and reads in Korean on the card.
    func testPlanRefusalsReadAsTheirKoreanSentence() throws {
        var fixture = try makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        func refusal(_ operation: HvfWindowsSnapshotCommand.Operation) -> String {
            do {
                _ = try HvfWindowsSnapshotCommand.plan(config: fixture.config, repoRoot: fixture.repo, operation: operation)
                XCTFail("the plan was refused")
                return ""
            } catch {
                XCTAssertFalse(error.localizedDescription.isEmpty)
                return HvfWindowsSnapshotModel.message(for: error, operation: operation)
            }
        }
        XCTAssertEqual(refusal(.restore), "되돌릴 스냅샷이 없으니 먼저 스냅샷을 만드세요.")

        let folder = fixture.bundle.appendingPathComponent("metadata/snapshots")
        try FileManager.default.createSymbolicLink(at: folder, withDestinationURL: fixture.root)
        XCTAssertEqual(refusal(.create), "VM 번들 안의 스냅샷 폴더를 안전하게 쓸 수 없어 스냅샷을 만들지 못했으니 "
                       + "그 폴더가 심볼릭 링크가 아닌 일반 폴더인지 확인하세요.")
        try FileManager.default.removeItem(at: folder)

        let helper = fixture.repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        try FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: helper.path)
        XCTAssertEqual(refusal(.create), "스냅샷 도우미 프로그램을 찾지 못해 스냅샷을 만들지 못했으니 BridgeVM을 다시 설치하세요.")

        let libraryOnly = "스냅샷은 BridgeVM 라이브러리에 있는 VM에서만 쓸 수 있으니 라이브러리에서 VM을 열어 다시 시도하세요."
        fixture.config.vtpmKeyID = nil
        XCTAssertEqual(refusal(.create), libraryOnly)
        fixture.config.targetDiskPath = fixture.root.appendingPathComponent("elsewhere.raw").path
        XCTAssertEqual(refusal(.restore), libraryOnly)
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
        XCTAssertTrue(FileManager.default.createFile(atPath: helper.path, contents: Data("#!/bin/sh\nprintf 'disk_bytes 128\\nvars_bytes 16\\n'\n".utf8)))
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
