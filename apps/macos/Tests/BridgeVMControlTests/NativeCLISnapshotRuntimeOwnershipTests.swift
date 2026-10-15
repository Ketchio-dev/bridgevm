import Foundation
import XCTest
@testable import BridgeVMControl

/// Drives the real snapshot helper while the media writer locks are held the
/// way an owned runtime holds them before its first guest generation and
/// between reboots, when no generation holds the pair lease.
final class NativeCLISnapshotRuntimeOwnershipTests: XCTestCase {
    func testPoweredOffOperationsRefuseWhileARuntimeHoldsTheMedia() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let baseline = fixture.run(.create)
        XCTAssertTrue(baseline.complete, baseline.unavailableReason ?? "")
        let manifest = fixture.snapshot.appendingPathComponent("manifest.json")
        let snapshot = try Data(contentsOf: manifest)
        try Data(repeating: 1, count: 64).write(to: fixture.disk)

        let locks = try [fixture.disk, fixture.vars].map(RuntimeWriterLock.init)
        let export = fixture.root.appendingPathComponent("exports/out.snapshot", isDirectory: true)
        for result in [fixture.run(.restore), fixture.run(.create), fixture.export(to: export)] {
            let reason = result.unavailableReason ?? ""
            XCTAssertFalse(result.complete, "\(result.command) admitted under a runtime")
            XCTAssertNil(result.snapshotPath)
            XCTAssertTrue(reason.contains("media is in use by a running VM"), reason)
            XCTAssertTrue(reason.contains("hvf-runner --owned-runtime-stdio"), reason)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: export.path))
        XCTAssertEqual(try Data(contentsOf: manifest), snapshot)

        locks.forEach { $0.release() }
        let restored = fixture.run(.restore)
        XCTAssertTrue(restored.complete, restored.unavailableReason ?? "")
    }

    /// The runtime's lease: flock(LOCK_EX) on `<image>.bridgevm-writer.lock`.
    private final class RuntimeWriterLock {
        private var descriptor: Int32

        init(_ image: URL) throws {
            descriptor = open(image.path + ".bridgevm-writer.lock", O_RDWR | O_CREAT, 0o600)
            guard descriptor >= 0 else { throw POSIXError(.EIO) }
            guard flock(descriptor, LOCK_EX | LOCK_NB) == 0 else {
                close(descriptor)
                throw POSIXError(.EWOULDBLOCK)
            }
            let holder = Data("hvf-runner --owned-runtime-stdio (pid \(getpid()))\n".utf8)
            _ = holder.withUnsafeBytes { write(descriptor, $0.baseAddress, $0.count) }
        }

        func release() {
            if descriptor >= 0 { close(descriptor) }
            descriptor = -1
        }

        deinit { release() }
    }

    private final class Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let disk: URL
        let vars: URL
        let snapshot: URL
        let id = "runtime-owned-vm"

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("native-cli-runtime-owned-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            let entry = library.appendingPathComponent(id, isDirectory: true)
            let bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
            disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            snapshot = bundle.appendingPathComponent("metadata/snapshots/latest.snapshot")
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(), helper.deletingLastPathComponent()] {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data(repeating: 7, count: 64).write(to: disk)
            try Data(repeating: 9, count: 4).write(to: vars)
            // A copy, not a link: the plan refuses a helper that is not canonical.
            try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
            let config = VMConfig(
                id: id, name: "Runtime VM", displayName: "Runtime VM", backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
                displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
                cpuCount: 4, experimental3DAllowed: false
            )
            try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
        }

        func run(_ operation: HvfWindowsSnapshotCommand.Operation) -> NativeCLISnapshotResult {
            NativeCLISnapshot.run(rootURL: library, id: id, operation: operation, repoRoot: repo)
        }

        func export(to output: URL) -> NativeCLISnapshotResult {
            NativeCLISnapshotExport.run(rootURL: library, options: .init(id: id, output: output), repoRoot: repo)
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
