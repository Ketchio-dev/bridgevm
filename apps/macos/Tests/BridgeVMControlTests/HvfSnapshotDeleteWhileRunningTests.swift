import XCTest
@testable import BridgeVMControl

/// Deleting a snapshot removes only its own folder, which a running VM never reads, so it frees space
/// while Windows runs. Create and restore still need Windows shut down.
@MainActor
final class HvfSnapshotDeleteWhileRunningTests: XCTestCase {
    func testASnapshotCanBeDeletedWhileWindowsRunsButNotRestored() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-delete-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = root.appendingPathComponent("library"), repo = root.appendingPathComponent("repo")
        let bundle = library.appendingPathComponent("snapshot-vm/bundle.vmbridge")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        for directory in [disk, vars, helper].map({ $0.deletingLastPathComponent() }) {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data(repeating: 7, count: 128).write(to: disk)
        try Data(repeating: 9, count: 16).write(to: vars)
        try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
        let config = try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "snapshot-vm", name: "Snapshot VM", displayName: "Snapshot VM", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
            displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
            cpuCount: 4, experimental3DAllowed: false), rootURL: library))
        let session = HvfEngineSession(config: config, repoRoot: repo) { _ in false }
        let model = session.snapshotModel
        await model.create { try await session.runSnapshot($0, configuration: config) }?.value
        XCTAssertEqual(model.status, "Snapshot created", session.events.last?.displayText ?? "")
        let snapshot = try XCTUnwrap(HvfWindowsSnapshotStore.list(bundle: bundle).first)
        let run: HvfWindowsSnapshotModel.Run = { try await session.runSnapshot($0, configuration: config, target: snapshot.url) }

        session.connectionState = .connected(host: "WIN")
        defer { session.connectionState = .stopped }
        let restore = try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: snapshot.date, run: run))
        await model.confirm(restore)?.value
        XCTAssertEqual(model.status, "Shut down the VM and wait for other operations to finish before restoring the snapshot.")
        let delete = try XCTUnwrap(model.confirmation(for: .delete, snapshotDate: snapshot.date, run: run))
        await model.confirm(delete)?.value
        XCTAssertEqual(model.status, "Snapshot deleted", session.events.last?.displayText ?? "")
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle), [])
        XCTAssertFalse(session.preparingSnapshot, "a Quit no longer waits")
    }
}
