import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotFailedCreateTests: XCTestCase {
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
            disk: disk, vars: vars, snapshot: legacy.deletingLastPathComponent().appendingPathComponent("20261112-000000.snapshot"), vmID: "test")
        do {
            _ = try await HvfWindowsSnapshotCommand.run(.create, plan: plan)
            XCTFail("helper must fail")
        } catch {
            XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle), before)
            XCTAssertFalse(FileManager.default.fileExists(atPath: plan.snapshot.path))
        }
    }
}
