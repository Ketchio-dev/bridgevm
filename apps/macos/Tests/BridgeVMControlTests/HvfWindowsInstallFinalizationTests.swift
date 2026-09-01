import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallFinalizationTests: XCTestCase {
    private struct InjectedCrash: Error {}

    func testEveryDurableBoundaryReconcilesIdempotentlyWithoutPartialRunnableVM() throws {
        for boundary in HvfWindowsInstallFinalizationBoundary.allCases {
            let fixture = try makeFixture()
            defer { fixture.remove() }
            XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
                plan: fixture.plan,
                faultInjector: { if $0 == boundary { throw InjectedCrash() } },
                secureBootSeeder: syntheticSeeder
            ), "boundary=\(boundary.rawValue)")

            let crashed = try readConfig(fixture.configURL)
            let committed = boundaryIndex(boundary) >= boundaryIndex(.configPublished)
            XCTAssertEqual(crashed.installPending, !committed, "boundary=\(boundary.rawValue)")
            XCTAssertEqual(HvfEngineConfig.libraryVM(crashed) != nil, committed)
            if committed { try assertPublishedArtifacts(fixture) }

            let first = HvfWindowsInstallFinalization.reconcile(
                config: crashed, libraryRoot: fixture.libraryRoot,
                secureBootSeeder: syntheticSeeder)
            XCTAssertNil(first.issue, "boundary=\(boundary.rawValue)")
            XCTAssertEqual(first.config.installPending, false)
            try assertPublishedArtifacts(fixture)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.pendingRequest.path))
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.journal.path))

            let second = HvfWindowsInstallFinalization.reconcile(
                config: first.config, libraryRoot: fixture.libraryRoot,
                secureBootSeeder: syntheticSeeder)
            XCTAssertEqual(second.config, first.config)
            XCTAssertNil(second.issue)
        }
    }

    func testVMLibraryScanResumesJournalAfterSecureBootWasStaged() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .secureBootStaged { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))

        let scan = VMLibrary.scan(rootURL: fixture.libraryRoot)

        XCTAssertEqual(scan.configs.first?.installPending, false)
        XCTAssertTrue(scan.issues.isEmpty)
        try assertPublishedArtifacts(fixture)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.journal.path))
    }

    func testCorruptJournalForcesPersistedConfigPendingAndReportsIssue() throws {
        let fixture = try makeFixture(installPending: false)
        defer { fixture.remove() }
        try FileManager.default.createDirectory(
            at: fixture.journal.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("not-json".utf8).write(to: fixture.journal)

        let scan = VMLibrary.scan(rootURL: fixture.libraryRoot)

        XCTAssertEqual(scan.configs.first?.installPending, true)
        XCTAssertEqual(try readConfig(fixture.configURL).installPending, true)
        XCTAssertTrue(scan.issues.contains { $0.message.contains("실행을 차단") })
    }

    func testMismatchedJournalRootFailsClosed() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .prepared { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
        var journal = try JSONDecoder().decode(
            HvfWindowsInstallFinalizationJournal.self, from: Data(contentsOf: fixture.journal))
        journal.libraryRoot = "/tmp/different-library"
        try JSONEncoder().encode(journal).write(to: fixture.journal, options: .atomic)

        let scan = VMLibrary.scan(rootURL: fixture.libraryRoot)

        XCTAssertEqual(scan.configs.first?.installPending, true)
        XCTAssertTrue(scan.issues.contains { $0.message.contains("실행을 차단") })
    }

    func testMissingSourceAfterPreparedJournalRemainsPending() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .prepared { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
        try FileManager.default.removeItem(at: fixture.sourceDisk)

        let result = HvfWindowsInstallFinalization.reconcile(
            config: try readConfig(fixture.configURL), libraryRoot: fixture.libraryRoot,
            secureBootSeeder: syntheticSeeder)

        XCTAssertEqual(result.config.installPending, true)
        XCTAssertNotNil(result.issue)
        XCTAssertNotNil(HvfWindowsInstallRequest.load(bundlePath: fixture.bundle.path))
    }

    func testChangedPendingRequestCannotBeCommitted() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .varsStaged { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
        let changed = HvfWindowsInstallRequest(
            isoPath: "/different.iso", diskGiB: 64,
            injectViogpu3d: false, driverPackageDir: nil)
        XCTAssertTrue(changed.save(bundlePath: fixture.bundle.path))

        let result = HvfWindowsInstallFinalization.reconcile(
            config: try readConfig(fixture.configURL), libraryRoot: fixture.libraryRoot,
            secureBootSeeder: syntheticSeeder)

        XCTAssertEqual(result.config.installPending, true)
        XCTAssertTrue(result.issue?.contains("실행을 차단") == true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.doneRequest.path))
    }

    func testReconcilerDoesNotRaceAnActiveFinalizerLock() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .prepared { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
        let lock = try HvfWindowsInstallDurability.TransactionLock(
            url: fixture.lock, nonBlocking: true)
        let current = try readConfig(fixture.configURL)

        let result = withExtendedLifetime(lock) {
            HvfWindowsInstallFinalization.reconcile(
                config: current, libraryRoot: fixture.libraryRoot,
                secureBootSeeder: syntheticSeeder)
        }

        XCTAssertEqual(result.config.installPending, true)
        XCTAssertNil(result.issue)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.journal.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.finalDisk.path))
    }

    func testLegacyDoneRequestIsRestoredForRetryWithoutPromotion() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        try FileManager.default.moveItem(at: fixture.pendingRequest, to: fixture.doneRequest)

        let scan = VMLibrary.scan(rootURL: fixture.libraryRoot)

        XCTAssertEqual(scan.configs.first?.installPending, true)
        XCTAssertNotNil(HvfWindowsInstallRequest.load(bundlePath: fixture.bundle.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.doneRequest.path))
        XCTAssertTrue(scan.issues.contains { $0.message.contains("재시도 가능한") })
    }

    func testLegacyCompletedConfigWithoutJournalIsUnchanged() throws {
        let fixture = try makeFixture(installPending: false)
        defer { fixture.remove() }

        let scan = VMLibrary.scan(rootURL: fixture.libraryRoot)

        XCTAssertEqual(scan.configs.first?.installPending, false)
        XCTAssertTrue(scan.issues.isEmpty)
    }

    func testSymlinkAtPublishedDiskPathFailsClosed() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == .configStaged { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
        let outside = fixture.root.appendingPathComponent("outside.raw")
        try Data(repeating: 7, count: 4096).write(to: outside)
        try FileManager.default.createDirectory(
            at: fixture.finalDisk.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: fixture.finalDisk, withDestinationURL: outside)

        let result = HvfWindowsInstallFinalization.reconcile(
            config: try readConfig(fixture.configURL), libraryRoot: fixture.libraryRoot,
            secureBootSeeder: syntheticSeeder)

        XCTAssertEqual(result.config.installPending, true)
        XCTAssertNotNil(result.issue)
        XCTAssertTrue(try fixture.finalDisk.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink == true)
    }

    private func makeFixture(installPending: Bool = true) throws -> Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bv-finalize-tests-\(UUID().uuidString)", isDirectory: true)
        let library = root.appendingPathComponent("library", isDirectory: true)
        let slug = "txn-\(UUID().uuidString.lowercased())"
        let bundle = root.appendingPathComponent("\(slug).vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
        let config = makeConfig(slug: slug, bundle: bundle, installPending: installPending)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        let request = HvfWindowsInstallRequest(
            isoPath: root.appendingPathComponent("windows.iso").path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        XCTAssertTrue(request.save(bundlePath: bundle.path))
        let plan = HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: library, bundlePath: bundle.path,
            slug: slug, request: request)
        try createSparseFile(URL(fileURLWithPath: plan.tmpTargetPath), bytes: 4096)
        try createSparseFile(URL(fileURLWithPath: plan.tmpVarsPath), bytes: 8192)
        return Fixture(root: root, libraryRoot: library, bundle: bundle, plan: plan)
    }

    private func makeConfig(slug: String, bundle: URL, installPending: Bool) -> VMConfig {
        VMConfig(
            id: slug, name: slug, displayName: "Transaction Test", backendKind: "hvf-engine",
            bootMode: "iso-efi", bundlePath: bundle.path, runnerPath: "/runner",
            launchSpecPath: bundle.appendingPathComponent("launch.json").path,
            handoffPath: bundle.appendingPathComponent("handoff.json").path,
            sshKeyPath: "/key", sshUser: "bridge", leasesPath: "/leases",
            guestName: "windows", displayWidth: 1280, displayHeight: 720,
            installPending: installPending, isoPath: nil, diskPath: nil,
            memMiB: 4096, cpuCount: 4, networkEnabled: true, experimental3DAllowed: false)
    }

    private func createSparseFile(_ url: URL, bytes: UInt64) throws {
        try? FileManager.default.removeItem(at: url)
        XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: bytes)
        try handle.close()
    }

    private func readConfig(_ url: URL) throws -> VMConfig {
        try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: url))
    }

    private func boundaryIndex(_ boundary: HvfWindowsInstallFinalizationBoundary) -> Int {
        HvfWindowsInstallFinalizationBoundary.allCases.firstIndex(of: boundary)!
    }

    private func assertPublishedArtifacts(_ fixture: Fixture) throws {
        XCTAssertEqual(try fileSize(fixture.finalDisk), 4096)
        XCTAssertEqual(try fileSize(fixture.finalVars), 8192)
        _ = try JSONDecoder().decode(
            HvfSecureBootProvisioningReceipt.self, from: Data(contentsOf: fixture.receipt))
        _ = try JSONDecoder().decode(
            HvfWindowsInstallRequest.self, from: Data(contentsOf: fixture.doneRequest))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.control.path))
    }

    private func fileSize(_ url: URL) throws -> UInt64 {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        return try XCTUnwrap(attributes[.size] as? NSNumber).uint64Value
    }

    private func syntheticSeeder(_ varsPath: String, _ diskPath: String) throws -> Data {
        let receipt = HvfSecureBootProvisioningReceipt(
            schemaVersion: 1, policy: "test-only", sourceTag: "test", sourceCommit: "test",
            sourceAssetSha256: String(repeating: "a", count: 64), firmwareFileName: "test.fd",
            firmwareSha256: String(repeating: "b", count: 64), firmwareEdk2Commit: "test",
            provisionedAt: "2026-09-01T00:00:00Z", variables: [])
        return try JSONEncoder().encode(receipt)
    }
}

private struct Fixture {
    let root: URL
    let libraryRoot: URL
    let bundle: URL
    let plan: HvfWindowsInstallPlan

    var configURL: URL { libraryRoot.appendingPathComponent(plan.slug).appendingPathComponent("vm.json") }
    var sourceDisk: URL { URL(fileURLWithPath: plan.tmpTargetPath) }
    var sourceVars: URL { URL(fileURLWithPath: plan.tmpVarsPath) }
    var journal: URL { bundle.appendingPathComponent("metadata/hvf-install-finalization/journal.json") }
    var lock: URL { journal.deletingLastPathComponent().appendingPathComponent("lock") }
    var finalDisk: URL { bundle.appendingPathComponent("disks/hvf-target.raw") }
    var finalVars: URL { bundle.appendingPathComponent("metadata/hvf-vars.fd") }
    var receipt: URL { bundle.appendingPathComponent("metadata/secure-boot-provisioning.json") }
    var pendingRequest: URL { bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName) }
    var doneRequest: URL { bundle.appendingPathComponent(HvfWindowsInstallRequest.doneFileName) }
    var control: URL { bundle.appendingPathComponent("metadata/hvf.ctl") }

    func remove() {
        try? FileManager.default.removeItem(at: root)
        try? FileManager.default.removeItem(at: sourceDisk)
        try? FileManager.default.removeItem(at: sourceVars)
    }
}
