import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotFailedCreateTests: XCTestCase {
    @MainActor
    func testUnwritableNameKeepsVerifiedSnapshotAndShowsSuccessNotice() async throws {
        let bundle = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-name-failure-\(UUID())").standardizedFileURL
        defer { try? FileManager.default.removeItem(at: bundle) }
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let snapshot = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("20261112-000000.snapshot")
        for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(), snapshot.deletingLastPathComponent()] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data(repeating: 7, count: 128).write(to: disk)
        try Data(repeating: 9, count: 16).write(to: vars)
        let sidecar = HvfWindowsSnapshotStore.nameFile(snapshot)
        try FileManager.default.createDirectory(at: sidecar, withIntermediateDirectories: true)
        let kept = sidecar.appendingPathComponent("keep.txt")
        try Data("keep".utf8).write(to: kept)
        let plan = HvfWindowsSnapshotCommand.Plan(executable: HvfMediaImportTestSupport.helper,
            disk: disk, vars: vars, snapshot: snapshot, vmID: "test", name: "이름")
        let model = HvfWindowsSnapshotModel()
        await model.request(.create, snapshotDate: nil) { _ in
            return try await HvfWindowsSnapshotCommand.run(.create, plan: plan)
        }?.value
        XCTAssertFalse(model.failed, "verified snapshot creation is successful even if naming fails")
        XCTAssertTrue(FileManager.default.fileExists(atPath: snapshot.appendingPathComponent("manifest.json").path))
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle).count, 1)
        XCTAssertNil(HvfWindowsSnapshotStore.list(bundle: bundle).first?.name)
        XCTAssertEqual(try Data(contentsOf: kept), Data("keep".utf8))
        XCTAssertEqual(model.status, "스냅샷은 생성했지만 이름을 저장하지 못해 날짜만 표시합니다.")
    }

    func testFailedHelperLeavesPublishedListUnchanged() async throws {
        let bundle = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-failure-\(UUID())")
        defer { try? FileManager.default.removeItem(at: bundle) }
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let legacy = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("latest.snapshot")
        try FileManager.default.createDirectory(at: legacy, withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: legacy.appendingPathComponent("manifest.json"))
        let before = HvfWindowsSnapshotStore.list(bundle: bundle)
        let plan = HvfWindowsSnapshotCommand.Plan(executable: URL(fileURLWithPath: "/usr/bin/false"),
            disk: disk, vars: vars, snapshot: legacy.deletingLastPathComponent().appendingPathComponent("20261112-000000.snapshot"), vmID: "test", name: "not published")
        do {
            _ = try await HvfWindowsSnapshotCommand.run(.create, plan: plan)
            XCTFail("helper must fail")
        } catch {
            XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle), before)
            XCTAssertFalse(FileManager.default.fileExists(atPath: plan.snapshot.path))
            XCTAssertFalse(FileManager.default.fileExists(atPath: HvfWindowsSnapshotStore.nameFile(plan.snapshot).path))
        }
    }
}
