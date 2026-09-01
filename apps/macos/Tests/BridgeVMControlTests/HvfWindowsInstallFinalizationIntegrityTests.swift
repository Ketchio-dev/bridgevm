import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallFinalizationIntegrityTests: XCTestCase {
    private struct InjectedCrash: Error {}

    func testV2JournalSealsSourceDiskAndVarsDigests() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        try stop(fixture, at: .prepared)

        let journal = try readJournal(fixture.journal)

        XCTAssertEqual(journal.schemaVersion, 2)
        XCTAssertEqual(journal.diskSHA256, HvfWindowsInstallCacheIdentity.sha256File(fixture.sourceDisk.path))
        XCTAssertEqual(journal.varsSHA256, HvfWindowsInstallCacheIdentity.sha256File(fixture.sourceVars.path))
        XCTAssertNil(journal.provisionedVarsSHA256)
    }

    func testV1JournalDecodesButFailsClosedBecauseItHasNoSourceDigests() throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        try stop(fixture, at: .prepared)
        let data = try Data(contentsOf: fixture.journal)
        guard var object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return XCTFail("journal must be an object") }
        object["schemaVersion"] = 1
        object.removeValue(forKey: "diskSHA256")
        object.removeValue(forKey: "varsSHA256")
        object.removeValue(forKey: "provisionedVarsSHA256")
        try JSONSerialization.data(withJSONObject: object).write(to: fixture.journal, options: .atomic)
        let decoded = try readJournal(fixture.journal)
        XCTAssertEqual(decoded.schemaVersion, 1)
        XCTAssertNil(decoded.diskSHA256)
        XCTAssertNil(decoded.varsSHA256)

        let result = try reconcile(fixture)

        XCTAssertEqual(result.config.installPending, true)
        XCTAssertTrue(result.issue?.contains("실행을 차단") == true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.journal.path))
    }

    func testSameSizeTamperOfStagedDiskAndVarsFailsClosed() throws {
        try assertTamperFailsClosed(boundary: .diskStaged, artifact: { $0.stagedDisk })
        try assertTamperFailsClosed(boundary: .varsStaged, artifact: { $0.stagedVars })
        try assertTamperFailsClosed(boundary: .secureBootStaged, artifact: { $0.stagedProvisionedVars })
    }

    func testSameSizeTamperOfSealedSourcesFailsClosedBeforeStaging() throws {
        try assertTamperFailsClosed(boundary: .prepared, artifact: { $0.sourceDisk })
        try assertTamperFailsClosed(boundary: .prepared, artifact: { $0.sourceVars })
    }

    func testSameSizeTamperOfPublishedDiskAndVarsFailsClosed() throws {
        try assertTamperFailsClosed(boundary: .diskPublished, artifact: { $0.finalDisk })
        try assertTamperFailsClosed(boundary: .varsPublished, artifact: { $0.finalVars })
    }

    private func assertTamperFailsClosed(boundary: HvfWindowsInstallFinalizationBoundary,
                                         artifact: (IntegrityFixture) -> URL) throws {
        let fixture = try makeFixture()
        defer { fixture.remove() }
        try stop(fixture, at: boundary)
        let url = artifact(fixture)
        let size = try fileSize(url)
        try tamperOneByte(url)
        XCTAssertEqual(try fileSize(url), size)

        let result = try reconcile(fixture)

        XCTAssertEqual(result.config.installPending, true, "boundary=\(boundary.rawValue)")
        XCTAssertTrue(result.issue?.contains("digest") == true, "boundary=\(boundary.rawValue)")
        XCTAssertEqual(try readConfig(fixture.configURL).installPending, true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.journal.path))
    }

    private func stop(
        _ fixture: IntegrityFixture, at boundary: HvfWindowsInstallFinalizationBoundary
    ) throws {
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(
            plan: fixture.plan,
            faultInjector: { if $0 == boundary { throw InjectedCrash() } },
            secureBootSeeder: syntheticSeeder))
    }

    private func reconcile(_ fixture: IntegrityFixture) throws -> HvfWindowsInstallFinalization.ReconcileResult {
        HvfWindowsInstallFinalization.reconcile(
            config: try readConfig(fixture.configURL), libraryRoot: fixture.libraryRoot,
            secureBootSeeder: syntheticSeeder)
    }

    private func makeFixture() throws -> IntegrityFixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bv-finalize-integrity-\(UUID().uuidString)", isDirectory: true)
        let library = root.appendingPathComponent("library", isDirectory: true)
        let slug = "integrity-\(UUID().uuidString.lowercased())"
        let bundle = root.appendingPathComponent("\(slug).vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
        let config = VMConfig(
            id: slug, name: slug, displayName: "Integrity Test", backendKind: "hvf-engine",
            bootMode: "iso-efi", bundlePath: bundle.path, runnerPath: "/runner",
            launchSpecPath: bundle.appendingPathComponent("launch.json").path,
            handoffPath: bundle.appendingPathComponent("handoff.json").path,
            sshKeyPath: "/key", sshUser: "bridge", leasesPath: "/leases",
            guestName: "windows", displayWidth: 1280, displayHeight: 720,
            installPending: true, isoPath: nil, diskPath: nil, memMiB: 4096, cpuCount: 4,
            networkEnabled: true, experimental3DAllowed: false)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        let request = HvfWindowsInstallRequest(
            isoPath: root.appendingPathComponent("windows.iso").path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        XCTAssertTrue(request.save(bundlePath: bundle.path))
        let plan = HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: library, bundlePath: bundle.path,
            slug: slug, request: request)
        try sparse(URL(fileURLWithPath: plan.tmpTargetPath), bytes: 4096)
        try sparse(URL(fileURLWithPath: plan.tmpVarsPath), bytes: 8192)
        return IntegrityFixture(root: root, libraryRoot: library, bundle: bundle, plan: plan)
    }

    private func sparse(_ url: URL, bytes: UInt64) throws {
        try? FileManager.default.removeItem(at: url)
        XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: bytes)
        try handle.close()
    }

    private func tamperOneByte(_ url: URL) throws {
        let handle = try FileHandle(forUpdating: url)
        defer { try? handle.close() }
        let original = try XCTUnwrap(try handle.read(upToCount: 1)?.first)
        try handle.seek(toOffset: 0)
        try handle.write(contentsOf: Data([original ^ 0xff]))
        try handle.synchronize()
    }

    private func fileSize(_ url: URL) throws -> UInt64 {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        return try XCTUnwrap(attributes[.size] as? NSNumber).uint64Value
    }

    private func readConfig(_ url: URL) throws -> VMConfig {
        try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: url))
    }

    private func readJournal(_ url: URL) throws -> HvfWindowsInstallFinalizationJournal {
        try JSONDecoder().decode(
            HvfWindowsInstallFinalizationJournal.self, from: Data(contentsOf: url))
    }

    private func syntheticSeeder(_ varsPath: String, _ diskPath: String) throws -> Data {
        var vars = try Data(contentsOf: URL(fileURLWithPath: varsPath))
        vars[0] ^= 0xa5
        try vars.write(to: URL(fileURLWithPath: varsPath))
        let receipt = HvfSecureBootProvisioningReceipt(
            schemaVersion: 1, policy: "test-only", sourceTag: "test", sourceCommit: "test",
            sourceAssetSha256: String(repeating: "a", count: 64), firmwareFileName: "test.fd",
            firmwareSha256: String(repeating: "b", count: 64), firmwareEdk2Commit: "test",
            provisionedAt: "2026-09-01T00:00:00Z", variables: [])
        return try JSONEncoder().encode(receipt)
    }
}

private struct IntegrityFixture {
    let root: URL
    let libraryRoot: URL
    let bundle: URL
    let plan: HvfWindowsInstallPlan
    var configURL: URL { libraryRoot.appendingPathComponent(plan.slug).appendingPathComponent("vm.json") }
    var sourceDisk: URL { URL(fileURLWithPath: plan.tmpTargetPath) }
    var sourceVars: URL { URL(fileURLWithPath: plan.tmpVarsPath) }
    var transaction: URL { bundle.appendingPathComponent("metadata/hvf-install-finalization") }
    var journal: URL { transaction.appendingPathComponent("journal.json") }
    var stagedDisk: URL { transaction.appendingPathComponent("disk.raw") }
    var stagedVars: URL { transaction.appendingPathComponent("vars.fd") }
    var stagedProvisionedVars: URL { transaction.appendingPathComponent("vars-provisioned.fd") }
    var finalDisk: URL { bundle.appendingPathComponent("disks/hvf-target.raw") }
    var finalVars: URL { bundle.appendingPathComponent("metadata/hvf-vars.fd") }
    func remove() {
        try? FileManager.default.removeItem(at: root)
        try? FileManager.default.removeItem(at: sourceDisk)
        try? FileManager.default.removeItem(at: sourceVars)
    }
}
