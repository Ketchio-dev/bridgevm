import XCTest
@testable import BridgeVMControl

final class LibraryNestedRelocationTests: XCTestCase {
    private struct Fixture {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("nested-move-" + UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }
        let fm = FileManager.default

        func config(_ id: String) -> VMConfig {
            let bundle = library.appendingPathComponent(id + "/bundle")
            return VMConfig(id: id, name: id, displayName: id, backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: id,
                displayWidth: 800, displayHeight: 600, installPending: false,
                diskPath: bundle.appendingPathComponent("disks/hvf-target.raw").path)
        }

        func create(_ id: String) throws -> VMConfig {
            let value = config(id)
            for (relative, bytes) in [("disks/hvf-target.raw", id + " disk"), ("metadata/hvf-vars.fd", id + " vars")] {
                let file = URL(fileURLWithPath: value.bundlePath).appendingPathComponent(relative)
                try fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
                try Data(bytes.utf8).write(to: file)
            }
            // No vTPM state: actual same-identity moves do not access the key store.
            XCTAssertTrue(VMLibrary.save(value, rootURL: library))
            return value
        }

        func assertReadable(_ config: VMConfig, id: String) throws {
            XCTAssertEqual(try? Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))), Data((id + " disk").utf8))
            XCTAssertEqual(try? Data(contentsOf: URL(fileURLWithPath: config.bundlePath)
                .appendingPathComponent("metadata/hvf-vars.fd")), Data((id + " vars").utf8))
            XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: id), config)
        }

        func remove() { try? fm.removeItem(at: root) }
    }

    func testMovingContainerRefusesToOrphanAnotherRegisteredBundle() throws {
        let f = Fixture(); defer { f.remove() }
        let owner = try f.create("owner")
        let child = try f.create("child")
        let nested = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(child,
            to: URL(fileURLWithPath: owner.bundlePath), rootURL: f.library))
        try f.assertReadable(nested, id: "child")
        let destination = f.root.appendingPathComponent("moved-owner")

        XCTAssertTrue(VMRelocationReferences.failureMessage(for: owner, rootURL: f.library).contains("다른 VM"))
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(owner, to: destination, rootURL: f.library))
        try f.assertReadable(owner, id: "owner")
        try f.assertReadable(nested, id: "child")
        XCTAssertFalse(VMRelocationJournal.isPending(owner, rootURL: f.library))
        XCTAssertFalse(f.fm.fileExists(atPath: destination.path))

        let rescued = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(nested,
            to: f.root.appendingPathComponent("rescued-child"), rootURL: f.library))
        let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(owner, to: destination, rootURL: f.library))
        try f.assertReadable(moved, id: "owner")
        try f.assertReadable(rescued, id: "child")
    }

    func testMovingOwnerWithUnrelatedRelocatedBundleStillSucceeds() throws {
        let f = Fixture(); defer { f.remove() }
        let owner = try f.create("owner")
        let child = try f.create("child")
        let outside = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(child,
            to: f.root.appendingPathComponent("outside-child"), rootURL: f.library))
        let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(owner,
            to: f.root.appendingPathComponent("moved-owner"), rootURL: f.library))

        try f.assertReadable(moved, id: "owner")
        try f.assertReadable(outside, id: "child")
        XCTAssertFalse(f.fm.fileExists(atPath: owner.bundlePath))
        XCTAssertFalse(VMRelocationJournal.isPending(moved, rootURL: f.library))
    }

    func testMovingBundleKeepsReferencesToOtherFilesInItsLibraryEntryValid() throws {
        let f = Fixture(); defer { f.remove() }
        let owner = try f.create("owner")
        var child = try f.create("child")
        let shared = f.library.appendingPathComponent("owner/outside-bundle.iso")
        let bytes = Data("entry sibling source".utf8)
        try bytes.write(to: shared)
        child.isoPath = shared.path
        XCTAssertTrue(VMLibrary.save(child, rootURL: f.library))

        let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(owner,
            to: f.root.appendingPathComponent("moved-owner"), rootURL: f.library))
        try f.assertReadable(moved, id: "owner")
        try f.assertReadable(child, id: "child")
        XCTAssertEqual(try Data(contentsOf: shared), bytes)
    }

    func testMovingBundleRefusesAnotherVMsInstallRequestSource() throws {
        let f = Fixture(); defer { f.remove() }
        let owner = try f.create("owner")
        var child = try f.create("child")
        child.installPending = true
        XCTAssertTrue(VMLibrary.save(child, rootURL: f.library))
        let request = HvfWindowsInstallRequest(isoPath: try XCTUnwrap(owner.diskPath), diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        XCTAssertTrue(request.save(bundlePath: child.bundlePath))
        let destination = f.root.appendingPathComponent("moved-owner")

        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(owner, to: destination, rootURL: f.library))
        try f.assertReadable(owner, id: "owner")
        try f.assertReadable(child, id: "child")
        XCTAssertEqual(HvfWindowsInstallRequest.load(bundlePath: child.bundlePath), request)
        XCTAssertFalse(VMRelocationJournal.isPending(owner, rootURL: f.library))
        XCTAssertFalse(f.fm.fileExists(atPath: destination.path))
    }

    @MainActor
    func testLibraryMoveRefusalUsesRealMediaLeaseAndExplainsSharedReference() async throws {
        let f = Fixture(); defer { f.remove() }
        let owner = try f.create("owner")
        let child = try f.create("child")
        let nested = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(child,
            to: URL(fileURLWithPath: owner.bundlePath), rootURL: f.library))
        let repository = f.root.appendingPathComponent("helper-repository")
        let helper = repository.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        let wrapper = repository.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        for file in [helper, wrapper] { try f.fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true) }
        try f.fm.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
        try Data("#!/bin/sh\nexit 99\n".utf8).write(to: wrapper)
        for file in [helper, wrapper] { try f.fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path) }
        let previous = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
        setenv("BRIDGEVM_REPO_ROOT", repository.path, 1)
        defer {
            if let previous { setenv("BRIDGEVM_REPO_ROOT", previous, 1) } else { unsetenv("BRIDGEVM_REPO_ROOT") }
        }
        var reachedOwnedOperation = false
        try HvfMediaLeaseSession.withOwnership(config: owner) { reachedOwnedOperation = true }
        XCTAssertTrue(reachedOwnedOperation)
        let jobs = NestedRelocationJobs()
        let model = LibraryModel(rootURL: f.library, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false)
        let destination = f.root.appendingPathComponent("moved-owner")
        model.moveWindowsHVFBundle(owner, to: destination)
        let job = try XCTUnwrap(jobs.pending.first)
        await job()

        let message = try XCTUnwrap(model.moveError)
        XCTAssertTrue(message.contains("다른 VM"), message)
        XCTAssertTrue(message.contains("등록 정보를 확인"), message)
        try f.assertReadable(owner, id: "owner")
        try f.assertReadable(nested, id: "child")
        XCTAssertFalse(f.fm.fileExists(atPath: destination.path))
        XCTAssertTrue(model.movingSlugs.isEmpty)
    }
}

@MainActor
private final class NestedRelocationJobs {
    var pending: [LibraryActionJob] = []
}
