import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsSnapshotCreationClaimTests: XCTestCase {
    func testTwoStalePlansCreateDistinctVerifiedSnapshots() async throws {
        let bundle = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-claim-\(UUID())").standardizedFileURL
        defer { try? FileManager.default.removeItem(at: bundle) }
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        for url in [disk, vars] {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(repeating: 7, count: 128).write(to: url)
        }
        // Both callers observed the same directory listing before either ran.
        let destination = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("20261112-120000.snapshot")
        let first = HvfWindowsSnapshotCommand.Plan(executable: HvfMediaImportTestSupport.helper,
            disk: disk, vars: vars, snapshot: destination, vmID: "test", name: "first")
        var second = first
        second.name = "second"
        _ = try await HvfWindowsSnapshotCommand.run(.create, plan: first)
        _ = try await HvfWindowsSnapshotCommand.run(.create, plan: second)
        let entries = HvfWindowsSnapshotStore.list(bundle: bundle)
        XCTAssertEqual(entries.count, 2)
        XCTAssertEqual(Set(entries.compactMap(\.name)), ["first", "second"])
        for entry in entries {
            XCTAssertNoThrow(try HvfWindowsSnapshotCommand.invoke(first.executable, ["verify", entry.url.path]))
        }
    }

    func testManifestlessInterruptedDirectoryIsHiddenAndDoesNotPreventNextCreate() async throws {
        let bundle = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-interrupted-\(UUID())").standardizedFileURL
        defer { try? FileManager.default.removeItem(at: bundle) }
        let destination = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("20261112-120000.snapshot")
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        XCTAssertTrue(HvfWindowsSnapshotStore.list(bundle: bundle).isEmpty)
        try Data("partial".utf8).write(to: destination.appendingPathComponent("disk.raw"))
        XCTAssertTrue(HvfWindowsSnapshotStore.list(bundle: bundle).isEmpty)
        let next = HvfWindowsSnapshotStore.nextURL(bundle: bundle,
            now: HvfWindowsSnapshotStore.timestamp(destination.lastPathComponent)!, occupied: [destination.lastPathComponent])
        XCTAssertEqual(next.lastPathComponent, "20261112-120001.snapshot")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        try FileManager.default.createDirectory(at: disk.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(repeating: 1, count: 128).write(to: disk)
        try Data(repeating: 2, count: 16).write(to: vars)
        let plan = HvfWindowsSnapshotCommand.Plan(executable: HvfMediaImportTestSupport.helper,
            disk: disk, vars: vars, snapshot: destination, vmID: "test")
        _ = try await HvfWindowsSnapshotCommand.run(.create, plan: plan)
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle).map(\.url.path), [next.path])
        XCTAssertEqual(try Data(contentsOf: destination.appendingPathComponent("disk.raw")), Data("partial".utf8))
    }
}
