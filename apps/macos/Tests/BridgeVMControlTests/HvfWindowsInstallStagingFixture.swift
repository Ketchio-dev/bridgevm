import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Media paths are read back from the scripted-installer command, not from plan
/// properties, so these checks bind what the runner is actually handed.
struct HvfWindowsInstallStagingFixture {
    struct InjectedCrash: Error {}
    typealias Media = (target: URL, vars: URL, evidence: URL)

    let root: URL
    let library: URL
    let bundle: URL
    let plan: HvfWindowsInstallPlan

    init(diskGiB: Int = 64) throws {
        root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("install-staging-" + UUID().uuidString, isDirectory: true)
        library = root.appendingPathComponent("library", isDirectory: true)
        let slug = "staging-" + UUID().uuidString.lowercased()
        bundle = root.appendingPathComponent("\(slug).vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(
            at: bundle.appendingPathComponent("metadata", isDirectory: true), withIntermediateDirectories: true)
        let request = HvfWindowsInstallRequest(
            isoPath: root.appendingPathComponent("windows.iso").path,
            diskGiB: diskGiB, injectViogpu3d: false, driverPackageDir: nil)
        plan = HvfWindowsInstallPlan(repoRoot: root, libraryRoot: library, bundlePath: bundle.path,
                                     slug: slug, request: request)
    }

    // The names an earlier build used; they may only be admitted from its journals.
    var legacyTarget: URL { URL(fileURLWithPath: "/tmp/bridgevm-appinstall-\(plan.slug)-target.raw") }
    var legacyVars: URL { URL(fileURLWithPath: "/tmp/bridgevm-appinstall-\(plan.slug)-vars.fd") }
    var legacyEvidence: URL { URL(fileURLWithPath: "/tmp/bridgevm-appinstall-\(plan.slug)-evidence") }
    var configURL: URL { library.appendingPathComponent(plan.slug).appendingPathComponent("vm.json") }
    var journalURL: URL { bundle.appendingPathComponent("metadata/hvf-install-finalization/journal.json") }
    var finalDisk: URL { bundle.appendingPathComponent("disks/hvf-target.raw") }

    func argument(_ flag: String) throws -> String {
        let command = plan.installCommand()
        let index = try XCTUnwrap(command.firstIndex(of: flag), "install command lacks \(flag)")
        XCTAssertLessThan(index + 1, command.count, "\(flag) has no value")
        return command[index + 1]
    }

    func media() throws -> Media {
        try (URL(fileURLWithPath: argument("--target")), URL(fileURLWithPath: argument("--vars")),
             URL(fileURLWithPath: argument("--evidence-dir"), isDirectory: true))
    }

    /// Persist the install-pending VM and request the finalizer requires.
    func register() throws {
        let config = VMConfig(
            id: plan.slug, name: plan.slug, displayName: "Staging Test", backendKind: "hvf-engine",
            bootMode: "iso-efi", bundlePath: bundle.path, runnerPath: "/runner",
            launchSpecPath: bundle.appendingPathComponent("launch.json").path,
            handoffPath: bundle.appendingPathComponent("handoff.json").path,
            sshKeyPath: "/key", sshUser: "bridge", leasesPath: "/leases",
            guestName: "windows", displayWidth: 1280, displayHeight: 720,
            installPending: true, isoPath: nil, diskPath: nil,
            memMiB: 4096, cpuCount: 4, networkEnabled: true, experimental3DAllowed: false)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        XCTAssertTrue(plan.request.save(bundlePath: bundle.path))
    }

    /// Write what a successful scripted install leaves where the command names it.
    func writeInstallerOutputs(log: Data) throws -> Media {
        let media = try self.media()
        for directory in [media.target.deletingLastPathComponent(), media.evidence]
        where !FileManager.default.fileExists(atPath: directory.path) {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        }
        XCTAssertTrue(FileManager.default.createFile(atPath: media.target.path, contents: nil))
        let handle = try FileHandle(forWritingTo: media.target)
        try handle.truncate(atOffset: 4096)
        try handle.close()
        try Data(repeating: 0x31, count: 8192).write(to: media.vars)
        try log.write(to: media.evidence.appendingPathComponent("run.log"))
        return media
    }

    func finalize(stoppingAt boundary: HvfWindowsInstallFinalizationBoundary) throws {
        try HvfWindowsInstallFinalization.finalize(plan: plan, faultInjector: {
            if $0 == boundary { throw InjectedCrash() }
        }, secureBootSeeder: Self.seeder)
    }

    /// A prepared journal as an earlier build sealed it, over the fixed shared-/tmp names.
    func makeLegacyJournal() throws {
        try register()
        let media = try writeInstallerOutputs(log: Data())
        XCTAssertThrowsError(try finalize(stoppingAt: .prepared))
        guard media.target.path != legacyTarget.path else { return }
        try FileManager.default.copyItem(at: media.target, to: legacyTarget)
        try FileManager.default.copyItem(at: media.vars, to: legacyVars)
        try FileManager.default.removeItem(at: media.target)
        try FileManager.default.removeItem(at: media.vars)
        var journal = try readJournal()
        journal.sourceDiskPath = legacyTarget.path
        journal.sourceVarsPath = legacyVars.path
        try writeJournal(journal)
    }

    func readJournal() throws -> HvfWindowsInstallFinalizationJournal {
        try JSONDecoder().decode(HvfWindowsInstallFinalizationJournal.self, from: Data(contentsOf: journalURL))
    }

    func writeJournal(_ journal: HvfWindowsInstallFinalizationJournal) throws {
        try JSONEncoder().encode(journal).write(to: journalURL, options: .atomic)
    }

    func reconcile() throws -> HvfWindowsInstallFinalization.ReconcileResult {
        let saved = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: configURL))
        return HvfWindowsInstallFinalization.reconcile(config: saved, libraryRoot: library,
                                                       secureBootSeeder: Self.seeder)
    }

    func remove() {
        try? FileManager.default.removeItem(at: root)
        for url in [legacyTarget, legacyVars, legacyEvidence] { try? FileManager.default.removeItem(at: url) }
    }

    static func isSharedTemporary(_ path: String) -> Bool {
        ["/tmp", "/private/tmp"].contains { path == $0 || path.hasPrefix($0 + "/") }
    }

    static func status(_ url: URL) -> stat? {
        var value = stat()
        return lstat(url.path, &value) == 0 ? value : nil
    }

    static func seeder(_ varsPath: String, _ diskPath: String) throws -> Data {
        let receipt = HvfSecureBootProvisioningReceipt(
            schemaVersion: 1, policy: "test-only", sourceTag: "test", sourceCommit: "test",
            sourceAssetSha256: String(repeating: "a", count: 64), firmwareFileName: "test.fd",
            firmwareSha256: String(repeating: "b", count: 64), firmwareEdk2Commit: "test",
            provisionedAt: "2026-09-01T00:00:00Z", variables: [])
        return try JSONEncoder().encode(receipt)
    }
}
