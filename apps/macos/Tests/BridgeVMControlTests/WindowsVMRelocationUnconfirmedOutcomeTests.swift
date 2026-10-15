import XCTest
@testable import BridgeVMControl

/// A move reported as unconfirmed may still have committed. Later actions must use
/// the persisted registration, never the pre-move configuration.
@MainActor
final class WindowsVMRelocationUnconfirmedOutcomeTests: XCTestCase {
    private final class FailingRollback: FileManager, @unchecked Sendable {
        var moves = 0
        override func moveItem(at source: URL, to destination: URL) throws {
            moves += 1
            if moves == 2 { throw CocoaError(.fileWriteNoPermission) }
            try super.moveItem(at: source, to: destination)
        }
    }

    @MainActor private final class Jobs { var pending: [LibraryActionJob] = [] }

    private final class Fixture {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("unconfirmed-move-" + UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }
        var source: URL { library.appendingPathComponent("move-fixture/bundle.vmbridge") }
        var parent: URL { root.appendingPathComponent("moved") }
        var destination: URL { parent.appendingPathComponent("bundle.vmbridge") }
        var config: VMConfig {
            VMConfig(id: "move-fixture", name: "Move", displayName: "Move",
                backendKind: "hvf-engine", bootMode: nil, bundlePath: source.path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
                sshUser: "", leasesPath: "", guestName: "move-fixture", displayWidth: 800, displayHeight: 600)
        }
        init(blockReceipt: Bool) throws {
            try FileManager.default.createDirectory(at: source.appendingPathComponent("metadata/vtpm"), withIntermediateDirectories: true)
            try Data("unchanged-media".utf8).write(to: source.appendingPathComponent("disk"))
            // A regular file makes lifecycle receipt directory creation fail after the bundle moved.
            if blockReceipt { try Data("blocked".utf8).write(to: source.appendingPathComponent("metadata/vtpm-lifecycle")) }
            XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        }
        deinit { try? FileManager.default.removeItem(at: root) }
        func assertRegistered(at bundle: URL, file: StaticString = #filePath, line: UInt = #line) throws {
            XCTAssertEqual(VMLibrary.list(rootURL: library).first?.bundlePath, bundle.path, file: file, line: line)
            XCTAssertEqual(try Data(contentsOf: destination.appendingPathComponent("disk")), Data("unchanged-media".utf8),
                           file: file, line: line)
        }
        /// The native owner grants the lease, then reports a failed release after the operation committed.
        func withReleaseFailingLease(_ body: () async -> Void) async throws {
            let repository = root.appendingPathComponent("helper-repository")
            let helper = repository.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            let wrapper = repository.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
            for url in [helper, wrapper] {
                try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            try Data("#!/bin/sh\nprintf 'bridgevm-media-lease-v1\\n'\nread command\nexit 7\n".utf8).write(to: helper)
            try Data("#!/bin/sh\nexit 99\n".utf8).write(to: wrapper)
            for url in [helper, wrapper] {
                try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
            }
            let prior = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
            setenv("BRIDGEVM_REPO_ROOT", repository.path, 1)
            defer { if let prior { setenv("BRIDGEVM_REPO_ROOT", prior, 1) } else { unsetenv("BRIDGEVM_REPO_ROOT") } }
            await body()
        }
    }

    func testLibraryUsesPersistedRegistrationAfterCommittedButUnconfirmedMove() async throws {
        let f = try Fixture(blockReceipt: false)
        let jobs = Jobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false,
            modelFactory: { config in
                ControlModel(config: config, backend: HvfWindowsBackend(config, processIsRunning: { _ in false },
                    libraryRoot: f.library), startsAutomatically: false)
            })
        let original = try XCTUnwrap(library.vms.first)
        let stale = library.model(for: original)

        library.moveWindowsHVFBundle(original, to: f.parent)
        let job = try XCTUnwrap(jobs.pending.first)
        try await f.withReleaseFailingLease { await job() }

        XCTAssertNotNil(library.moveError, "an unconfirmed move is still reported")
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.source.path))
        try f.assertRegistered(at: f.destination)
        let listed = try XCTUnwrap(library.vms.first { $0.slug == original.slug })
        XCTAssertEqual(listed.bundlePath, f.destination.path, "the library must reload the persisted registration")
        XCTAssertNotNil(stale.workAdmission?(false), "the pre-move control handle must no longer be admitted")
        let model = library.model(for: listed)
        XCTAssertFalse(model === stale, "the pre-move control model must be replaced")

        model.pendingMemGiB = 8
        model.pendingCPU = 4
        model.applyResources()
        let deadline = Date().addingTimeInterval(5)
        while model.lifecycleBusy, Date() < deadline { try await Task.sleep(nanoseconds: 10_000_000) }
        XCTAssertFalse(model.lifecycleBusy)
        try f.assertRegistered(at: f.destination)
        XCTAssertEqual(VMLibrary.list(rootURL: f.library).first?.memMiB, 8192)
    }

    func testStaleBackendDoesNotRevertRegistrationAfterFailedRollback() throws {
        let f = try Fixture(blockReceipt: true)
        let stale = HvfWindowsBackend(f.config, processIsRunning: { _ in false }, libraryRoot: f.library)
        let manager = FailingRollback()
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(f.config, to: f.parent, rootURL: f.library, fileManager: manager))
        XCTAssertEqual(manager.moves, 2)
        try f.assertRegistered(at: f.destination)

        XCTAssertFalse(stale.setResources(memMiB: 8192, cpu: 4), "a pre-move handle must not rewrite the registration")
        try f.assertRegistered(at: f.destination)
        XCTAssertEqual(stale.config, f.config)
    }
}
