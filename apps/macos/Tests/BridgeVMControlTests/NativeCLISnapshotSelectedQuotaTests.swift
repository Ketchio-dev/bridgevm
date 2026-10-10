import Foundation
import XCTest
@testable import BridgeVMControl

/// Uses the shipped Rust helper: restore selects a managed generation without
/// changing the logical originals, so their sizes must not set the create quota.
final class NativeCLISnapshotSelectedQuotaTests: XCTestCase {
    func testOriginalPairCanBeCreatedAndExportedAtItsExactSize() throws {
        let fixture = try Fixture(saved: .large)
        defer { fixture.remove() }

        try assertQuota(fixture.plan(), equals: "144", export: fixture.output)
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .large)
        try assertSnapshot(fixture.export(), at: fixture.output, matches: .large)
        try assertPair(.large, disk: fixture.disk, vars: fixture.vars)
    }

    func testLargerSelectedPairCanBeExportedAndCreatedAfterRepeatedRestores() throws {
        let fixture = try Fixture(saved: .large)
        defer { fixture.remove() }
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .large)
        try fixture.writeOriginals(.small)
        let plan = try fixture.plan()
        try assertQuota(plan, equals: "72", export: fixture.output)

        for attempt in 1...3 {
            let restored = fixture.run(.restore)
            XCTAssertTrue(restored.complete, "restore \(attempt): \(restored.unavailableReason ?? "")")
            // Even a plan formed before restore must size the selected pair now.
            try assertQuota(plan, equals: "144", export: fixture.output)
            try assertSnapshot(fixture.export(), at: fixture.output, matches: .large)
            try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .large)
            try assertPair(.small, disk: fixture.disk, vars: fixture.vars)
        }
    }

    func testSmallerSelectedPairNeverUsesLargerStaleOriginalsToInflateQuota() throws {
        let fixture = try Fixture(saved: .small)
        defer { fixture.remove() }
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .small)
        try fixture.writeOriginals(.large)
        let plan = try fixture.plan()
        try assertQuota(plan, equals: "144", export: fixture.output)

        let restored = fixture.run(.restore)
        XCTAssertTrue(restored.complete, restored.unavailableReason ?? "")
        try assertQuota(plan, equals: "72", export: fixture.output)
        try assertSnapshot(fixture.export(), at: fixture.output, matches: .small)
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .small)
        try assertQuota(plan, equals: "72", export: fixture.output)
        try assertPair(.large, disk: fixture.disk, vars: fixture.vars)
    }

    func testPreviouslyCalculatedQuotaStillRejectsALargerNewSelection() throws {
        let fixture = try Fixture(saved: .large)
        defer { fixture.remove() }
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .large)
        try fixture.writeOriginals(.small)
        let plan = try fixture.plan()
        let arguments = try plan.arguments(for: .create)
        XCTAssertEqual(arguments.last, "72")
        let manifest = fixture.snapshot.appendingPathComponent("manifest.json")
        let originalManifest = try Data(contentsOf: manifest)

        let restored = fixture.run(.restore)
        XCTAssertTrue(restored.complete, restored.unavailableReason ?? "")
        XCTAssertThrowsError(try HvfWindowsSnapshotCommand.invoke(plan.executable, arguments)) { error in
            XCTAssertTrue(error.localizedDescription.contains("144 bytes, over the 72 byte quota"),
                          error.localizedDescription)
        }
        XCTAssertEqual(try Data(contentsOf: manifest), originalManifest)
        try assertPair(.large, disk: fixture.snapshot.appendingPathComponent("disk.raw"),
                       vars: fixture.snapshot.appendingPathComponent("vars.fd"))
        try assertQuota(plan, equals: "144", export: fixture.output)
        try assertSnapshot(fixture.export(), at: fixture.output, matches: .large)
        try assertSnapshot(fixture.run(.create), at: fixture.snapshot, matches: .large)
        try assertPair(.small, disk: fixture.disk, vars: fixture.vars)
    }

    private func assertQuota(
        _ plan: HvfWindowsSnapshotCommand.Plan, equals bytes: String, export output: URL,
        file: StaticString = #filePath, line: UInt = #line
    ) throws {
        XCTAssertEqual(try plan.arguments(for: .create).last, bytes, file: file, line: line)
        XCTAssertEqual(try plan.createArguments(destination: output).last, bytes, file: file, line: line)
    }

    private func assertSnapshot(
        _ result: NativeCLISnapshotResult, at destination: URL, matches pair: Pair,
        file: StaticString = #filePath, line: UInt = #line
    ) throws {
        XCTAssertTrue(result.complete, result.unavailableReason ?? "", file: file, line: line)
        XCTAssertEqual(result.snapshotPath, destination.path, file: file, line: line)
        try assertPair(pair, disk: destination.appendingPathComponent("disk.raw"),
                       vars: destination.appendingPathComponent("vars.fd"), file: file, line: line)
    }

    private func assertPair(
        _ pair: Pair, disk: URL, vars: URL, file: StaticString = #filePath, line: UInt = #line
    ) throws {
        XCTAssertEqual(try Data(contentsOf: disk), pair.disk, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: vars), pair.vars, file: file, line: line)
    }

    private struct Pair {
        let disk: Data
        let vars: Data
        static let large = Pair(disk: Data(repeating: 7, count: 128), vars: Data(repeating: 9, count: 16))
        static let small = Pair(disk: Data(repeating: 3, count: 64), vars: Data(repeating: 5, count: 8))
    }

    private final class Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let disk: URL
        let vars: URL
        let snapshot: URL
        let output: URL
        let config: VMConfig
        let id = "selected-quota-vm"

        init(saved: Pair) throws {
            root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
                .appendingPathComponent("native-cli-selected-quota-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            let entry = library.appendingPathComponent(id, isDirectory: true)
            let bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
            disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            snapshot = bundle.appendingPathComponent("metadata/snapshots/latest.snapshot", isDirectory: true)
            output = root.appendingPathComponent("exports/current.snapshot", isDirectory: true)
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            config = VMConfig(
                id: id, name: "Selected quota VM", displayName: "Selected quota VM", backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
                displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
                cpuCount: 4, experimental3DAllowed: false
            )
            do {
                for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(),
                                  helper.deletingLastPathComponent()] {
                    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                }
                try writeOriginals(saved)
                // A real executable copy, not a stand-in or a noncanonical symlink.
                try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
                try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
            } catch {
                remove()
                throw error
            }
        }

        func writeOriginals(_ pair: Pair) throws {
            try pair.disk.write(to: disk)
            try pair.vars.write(to: vars)
        }

        func plan() throws -> HvfWindowsSnapshotCommand.Plan {
            let engine = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: library))
            return try HvfWindowsSnapshotCommand.plan(config: engine, repoRoot: repo, operation: .create)
        }

        func run(_ operation: HvfWindowsSnapshotCommand.Operation) -> NativeCLISnapshotResult {
            NativeCLISnapshot.run(rootURL: library, id: id, operation: operation, repoRoot: repo)
        }

        func export() -> NativeCLISnapshotResult {
            NativeCLISnapshotExport.run(rootURL: library, options: .init(id: id, output: output), repoRoot: repo)
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
