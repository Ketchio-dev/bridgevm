import Foundation
import XCTest
@testable import BridgeVMControl

private struct LibraryStartupRecoveryInterruption: Error {}

struct LibraryStartupRecoveryFixture {
    let root: URL
    let library: URL
    let plan: HvfWindowsInstallPlan
    let disk = Data(repeating: 0x41, count: 4096)
    let vars = Data(repeating: 0x31, count: 8192)
    var paths: HvfWindowsInstallFinalizationPaths {
        .init(libraryRoot: library, bundle: URL(fileURLWithPath: plan.bundlePath), slug: plan.slug)
    }

    init() throws {
        root = LibraryDeletionTestDirectory.temporary.resolvingSymlinksInPath()
            .appendingPathComponent("startup-recovery-" + UUID().uuidString)
        library = root.appendingPathComponent("library")
        let bundle = root.appendingPathComponent("bundle")
        let config = VMConfig(id: "owned", name: "Owned", displayName: "Owned", backendKind: "hvf-engine",
            bootMode: "iso-efi", bundlePath: bundle.path, runnerPath: "/unused", launchSpecPath: "/unused",
            handoffPath: "/unused", sshKeyPath: "/unused", sshUser: "fixture", leasesPath: "/unused",
            guestName: "fixture", displayWidth: 1280, displayHeight: 720, installPending: true,
            memMiB: 4096, cpuCount: 4, networkEnabled: false, experimental3DAllowed: false)
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("never-opened.iso").path,
            diskGiB: 64, injectViogpu3d: false)
        plan = HvfWindowsInstallPlan(repoRoot: root, libraryRoot: library, bundlePath: bundle.path,
            slug: config.slug, request: request)
        try FileManager.default.createDirectory(atPath: plan.stagingDirectory, withIntermediateDirectories: true)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        XCTAssertTrue(request.save(bundlePath: bundle.path))
    }

    func stageInterruption() throws {
        try disk.write(to: URL(fileURLWithPath: plan.stagingTargetPath))
        try vars.write(to: URL(fileURLWithPath: plan.stagingVarsPath))
        XCTAssertThrowsError(try HvfWindowsInstallFinalization.finalize(plan: plan,
            faultInjector: { if $0 == .secureBootStaged { throw LibraryStartupRecoveryInterruption() } },
            secureBootSeeder: { _, _ in
                try JSONEncoder().encode(HvfSecureBootProvisioningReceipt(schemaVersion: 1,
                    policy: "test-only", sourceTag: "test", sourceCommit: "test",
                    sourceAssetSha256: String(repeating: "a", count: 64), firmwareFileName: "test.fd",
                    firmwareSha256: String(repeating: "b", count: 64), firmwareEdk2Commit: "test",
                    provisionedAt: "2026-10-14T00:00:00Z", variables: []))
            }))
    }

    func clean() { try? FileManager.default.removeItem(at: root) }
}
