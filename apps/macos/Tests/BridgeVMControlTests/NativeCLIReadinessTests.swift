import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIReadinessTests: XCTestCase {
    private func fixture(backend: String = "hvf-engine", pending: Bool? = nil) throws -> (URL, VMConfig) {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("native-readiness-" + UUID().uuidString)
        let entry = root.appendingPathComponent("개발-vm")
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        var config = VMConfig(id: "stale-persisted-id", name: "개발", displayName: "개발 Windows",
            backendKind: backend, bootMode: "windows-hvf",
            bundlePath: entry.appendingPathComponent("bundle.vmbridge").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "Windows", displayWidth: 1280, displayHeight: 800,
            installPending: pending, memMiB: 6144, cpuCount: 4, experimental3DAllowed: false)
        try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
        config.id = "개발-vm"
        return (root, config)
    }

    private func contents(_ root: URL) throws -> [String: Data] {
        let names = try XCTUnwrap(FileManager.default.enumerator(atPath: root.path)?.allObjects as? [String])
        return try Dictionary(uniqueKeysWithValues: names.sorted().map { name in
            let url = root.appendingPathComponent(name)
            let directory = try url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true
            return (name + (directory ? "/" : ""), directory ? Data() : try Data(contentsOf: url))
        })
    }

    func testReadinessOptionsAcceptExactUnicodeIDAndFlagsInAnyOrder() throws {
        let root = URL(fileURLWithPath: "/private/tmp/native-readiness-missing", isDirectory: true)
        for args in [["readiness", "개발-vm", "--json", "--library", root.path],
                     ["--library", root.path, "readiness", "--json", "개발-vm"]] {
            let options = try NativeCLIOptions.parse(arguments: args)
            XCTAssertEqual(options.command, .readiness("개발-vm"))
            XCTAssertEqual(options.libraryRoot, root)
            XCTAssertTrue(options.json)
            XCTAssertFalse(options.showHelp)
        }
        for args in [["readiness"], ["readiness", "../vm"], ["readiness", "Not Canonical"],
                     ["readiness", "vm", "extra"], ["readiness", "vm", "--json", "--json"]] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: args))
        }
    }

    func testHelpPreservesProtocolMarkerAndExplainsPrerequisiteLimit() throws {
        let (root, _) = try fixture()
        let missing = root.appendingPathComponent("missing")
        let before = try contents(root)
        let options = try NativeCLIOptions.parse(arguments: ["readiness", "--help"], defaultLibrary: missing)
        XCTAssertTrue(options.showHelp)
        XCTAssertTrue(NativeCLI.help.contains("bridgevm-native-cli-v1"))
        XCTAssertTrue(NativeCLI.help.contains("bridgevm.app-readiness.v1"))
        XCTAssertTrue(NativeCLI.help.contains("prove guest behavior or release readiness"))
        XCTAssertEqual(try contents(root), before)
    }

    func testMissingPrerequisitesPreserveAppIssueCodesScopesAndSummariesWithoutWrites() throws {
        let (root, config) = try fixture()
        let repo = root.appendingPathComponent("absent-repo")
        let before = try contents(root)
        let app = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: root)).readiness(repoRoot: repo)
        let result = try NativeCLIReadiness.snapshot(rootURL: root, id: config.slug, repoRoot: repo)
        XCTAssertEqual(result.id, "개발-vm")
        XCTAssertTrue(result.engineChecksPerformed)
        XCTAssertFalse(result.launchReady)
        XCTAssertEqual(result.launchBlockers, app.launchBlockers.map(NativeCLIReadinessIssue.init))
        XCTAssertEqual(result.releaseBlockers, app.releaseBlockers.map(NativeCLIReadinessIssue.init))
        XCTAssertEqual(result.productLimitations, app.productLimitations)
        XCTAssertTrue(result.launchBlockers.contains { $0.code == "target-disk-missing" })
        XCTAssertTrue(result.launchBlockers.contains { $0.code == "uefi-vars-missing" })
        XCTAssertTrue(result.releaseBlockers.allSatisfy { $0.scope == "release" })
        XCTAssertTrue(result.launchBlockers.allSatisfy { $0.scope == "launch" })
        XCTAssertFalse(result.releaseBlockers.isEmpty)
        XCTAssertEqual(try contents(root), before)
        let encoded = try JSONEncoder().encode(result)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        XCTAssertEqual(object["schema"] as? String, "bridgevm.app-readiness.v1")
        XCTAssertEqual(object["runtimeState"] as? String, "unobserved")
        XCTAssertEqual(object["launchReady"] as? Bool, false)
        XCTAssertNil(object["releaseReady"])
        XCTAssertNil(object["raw_config"])
        let text = NativeCLI.render(result)
        XCTAssertTrue(text.contains("Launch blocker [target-disk-missing]"))
        XCTAssertTrue(text.contains("Product release gate [secure-boot-live-receipt]"))
    }

    func testPendingInstallationIsBlockedBeforeInstalledEngineChecks() throws {
        let (root, config) = try fixture(pending: true)
        let before = try contents(root)
        let result = try NativeCLIReadiness.snapshot(rootURL: root, id: config.slug, repoRoot: root)
        XCTAssertFalse(result.launchReady)
        XCTAssertFalse(result.engineChecksPerformed)
        XCTAssertEqual(result.launchBlockers.map(\.code), ["installation-pending"])
        XCTAssertTrue(NativeCLI.render(result).contains("checks were not evaluated"))
        XCTAssertEqual(try contents(root), before)
    }

    func testOtherAndUnknownRawBackendsAreExplicitlyUnsupported() throws {
        for backend in ["fast-vz", "qemu-compat", "future-hvf", "HVF-ENGINE"] {
            let (root, config) = try fixture(backend: backend)
            let before = try contents(root)
            let result = try NativeCLIReadiness.snapshot(rootURL: root, id: config.slug, repoRoot: root)
            XCTAssertEqual(result.backendKind, backend)
            XCTAssertFalse(result.launchReady)
            XCTAssertFalse(result.engineChecksPerformed)
            XCTAssertEqual(result.launchBlockers.map(\.code), ["unsupported-backend"])
            XCTAssertEqual(try contents(root), before)
        }
    }

    func testRelocationBlocksWhileFinalizationPresenceRemainsUnexaminedAndUnmodified() throws {
        let (root, config) = try fixture()
        let marker = root.appendingPathComponent(config.slug).appendingPathComponent(".relocation-pending.json")
        try Data("unexamined relocation record".utf8).write(to: marker)
        let paths = HvfWindowsInstallFinalizationPaths(libraryRoot: root,
            bundle: URL(fileURLWithPath: config.bundlePath), slug: config.slug)
        try FileManager.default.createDirectory(at: paths.transaction, withIntermediateDirectories: true)
        try Data("{\"phase\":12}".utf8).write(to: paths.journal)
        let before = try contents(root)
        let result = try NativeCLIReadiness.snapshot(rootURL: root, id: config.slug, repoRoot: root)
        XCTAssertFalse(result.launchReady)
        XCTAssertTrue(result.launchBlockers.contains { $0.code == "relocation-pending" })
        XCTAssertEqual(result.recoveryObservations, ["relocation-record-present-or-unreadable",
                                                     "install-finalization-record-present-or-unreadable"])
        XCTAssertFalse(result.launchBlockers.contains { $0.code.contains("finalization") })
        XCTAssertEqual(try contents(root), before)
    }

    func testMissingConfigurationAndUnsafeIDFailWithoutCreatingAnything() throws {
        let (root, _) = try fixture()
        let before = try contents(root)
        XCTAssertThrowsError(try NativeCLIReadiness.snapshot(rootURL: root, id: "missing", repoRoot: root))
        XCTAssertThrowsError(try NativeCLIReadiness.snapshot(rootURL: root, id: "../vm", repoRoot: root))
        XCTAssertThrowsError(try NativeCLIReadiness.snapshot(rootURL: root.appendingPathComponent("missing"),
                                                          id: "vm", repoRoot: root))
        XCTAssertEqual(try contents(root), before)
    }
}
