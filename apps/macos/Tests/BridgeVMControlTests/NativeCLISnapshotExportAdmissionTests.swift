import Foundation
import XCTest
@testable import BridgeVMControl

/// Drives the real snapshot helper, so the output admission under test is the
/// Rust code the app ships rather than a scripted stand-in.
final class NativeCLISnapshotExportAdmissionTests: XCTestCase {
    func testExportIntoUnrelatedDirectoryRefusesAndLeavesItIntact() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let personal = fixture.root.appendingPathComponent("exports/personal", isDirectory: true)
        try FileManager.default.createDirectory(at: personal, withIntermediateDirectories: true)
        let keep = personal.appendingPathComponent("keep.txt")
        try Data("irreplaceable non-VM data".utf8).write(to: keep)

        let result = fixture.export(to: personal)
        XCTAssertEqual(try? Data(contentsOf: keep), Data("irreplaceable non-VM data".utf8))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: personal.path), ["keep.txt"])
        XCTAssertFalse(result.complete, "export replaced an unrelated directory")
        XCTAssertNil(result.snapshotPath)
        XCTAssertTrue(result.unavailableReason?.contains("left intact: remove or rename it") == true, result.unavailableReason ?? "")
        let staging = personal.deletingLastPathComponent().appendingPathComponent(".personal.staging")
        XCTAssertFalse(FileManager.default.fileExists(atPath: staging.path))
    }

    func testExportReplacesItsOwnPreviousSnapshot() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let output = fixture.root.appendingPathComponent("exports/current.snapshot", isDirectory: true)
        for attempt in 1...2 {
            let result = fixture.export(to: output)
            XCTAssertTrue(result.complete, "attempt \(attempt): \(result.unavailableReason ?? "")")
        }
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: output.path).sorted(),
            ["disk.raw", "manifest.json", "vars.fd"]
        )
    }

    private final class Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let id = "admission-vm"

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("native-cli-export-admission-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            let entry = library.appendingPathComponent(id, isDirectory: true)
            let bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
            let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(), helper.deletingLastPathComponent()] {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data(repeating: 7, count: 64).write(to: disk)
            try Data(repeating: 9, count: 4).write(to: vars)
            // A copy, not a link: the plan refuses a helper that is not canonical.
            try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
            let config = VMConfig(
                id: id, name: "Admission VM", displayName: "Admission VM", backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
                displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
                cpuCount: 4, experimental3DAllowed: false
            )
            try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
        }

        func export(to output: URL) -> NativeCLISnapshotResult {
            NativeCLISnapshotExport.run(rootURL: library, options: .init(id: id, output: output), repoRoot: repo)
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
