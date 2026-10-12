import Foundation
import XCTest
@testable import BridgeVMControl

/// An interrupted move leaves its relocation record. The app neither loads nor
/// launches that VM, so no app snapshot can reach it; the CLI must also refuse
/// before touching either bundle.
final class NativeCLISnapshotRelocationAdmissionTests: XCTestCase {
    func testSnapshotCommandsRefuseUnresolvedRelocationWithoutTouchingMedia() throws {
        let f = try Fixture()
        addTeardownBlock { f.remove() }
        let baseline = f.run(.create)
        XCTAssertTrue(baseline.complete, baseline.unavailableReason ?? "")

        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(f.config, to: f.target, rootURL: f.library,
                                                   fileManager: PartialMove()))
        XCTAssertTrue(VMRelocationJournal.isPending(f.config, rootURL: f.library))
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.target.appendingPathComponent("bundle.vmbridge").path))
        XCTAssertFalse(VMLibrary.scan(rootURL: f.library).configs.contains { $0.slug == f.id },
                       "the app does not load the VM, so no app snapshot can be reserved")
        XCTAssertTrue(HvfLibraryLaunchContext(config: f.config, rootURL: f.library)
            .readinessIssues.contains { $0.code == "relocation-pending" })

        let export = f.root.appendingPathComponent("exports/out.snapshot", isDirectory: true)
        let before = try f.tree()
        for result in [f.run(.restore), f.run(.create), f.export(to: export)] {
            let reason = result.unavailableReason ?? ""
            XCTAssertFalse(result.complete, "\(result.command) admitted during an unresolved relocation")
            XCTAssertNil(result.snapshotPath)
            XCTAssertTrue(reason.contains("relocation"), reason)
        }
        XCTAssertEqual(try f.tree(), before, "refusal must not create, modify or remove files")

        // Control: once the record is resolved, restore publishes the managed pair.
        try FileManager.default.removeItem(at: VMRelocationJournal.url(f.config, rootURL: f.library))
        let restored = f.run(.restore)
        XCTAssertTrue(restored.complete, restored.unavailableReason ?? "")
        XCTAssertTrue(try f.tree().keys.contains { $0.contains("disks/.bridgevm-pair-v2-") })
    }

    /// The copy starts, then fails before the source is removed.
    private final class PartialMove: FileManager, @unchecked Sendable {
        override func moveItem(at source: URL, to destination: URL) throws {
            try createDirectory(at: destination, withIntermediateDirectories: true)
            throw CocoaError(.fileWriteUnknown)
        }
    }

    private final class Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let target: URL
        let disk: URL
        let config: VMConfig
        let id = "relocating-vm"

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("native-cli-relocation-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            target = root.appendingPathComponent("target", isDirectory: true)
            let bundle = root.appendingPathComponent("source/bundle.vmbridge", isDirectory: true)
            disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(), helper.deletingLastPathComponent()] {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data(repeating: 7, count: 64).write(to: disk)
            try Data(repeating: 9, count: 4).write(to: vars)
            try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
            config = VMConfig(
                id: id, name: "Relocating VM", displayName: "Relocating VM", backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
                displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
                cpuCount: 4, experimental3DAllowed: false
            )
            guard VMLibrary.save(config, rootURL: library) else { throw CocoaError(.fileWriteUnknown) }
        }

        func run(_ operation: HvfWindowsSnapshotCommand.Operation) -> NativeCLISnapshotResult {
            NativeCLISnapshot.run(rootURL: library, id: id, operation: operation, repoRoot: repo)
        }

        func export(to output: URL) -> NativeCLISnapshotResult {
            NativeCLISnapshotExport.run(rootURL: library, options: .init(id: id, output: output), repoRoot: repo)
        }

        /// Relative path, size and modification time of every entry under the fixture.
        func tree() throws -> [String: String] {
            var entries: [String: String] = [:]
            let enumerator = FileManager.default.enumerator(atPath: root.path)
            while let path = enumerator?.nextObject() as? String {
                let info = try FileManager.default.attributesOfItem(atPath: root.appendingPathComponent(path).path)
                let stamp = (info[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
                entries[path] = "\(info[.size] ?? -1)@\(stamp)"
            }
            return entries
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
