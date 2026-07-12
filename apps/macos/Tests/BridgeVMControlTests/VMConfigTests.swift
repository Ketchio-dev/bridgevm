import XCTest
@testable import BridgeVMControl

final class VMConfigSlugifyTests: XCTestCase {
    func testBasicSlugify() {
        XCTAssertEqual(VMConfig.slugify("Ubuntu 2"), "ubuntu-2")
        XCTAssertEqual(VMConfig.slugify("Fedora 40"), "fedora-40")
    }

    func testCollapsesAndTrimsSeparators() {
        XCTAssertEqual(VMConfig.slugify("  --My  VM--  "), "my-vm")
        XCTAssertEqual(VMConfig.slugify("a..b__c"), "a-b-c")
        XCTAssertEqual(VMConfig.slugify("saved/vm"), "saved-vm")
        XCTAssertNotEqual(VMConfig.slugify("saved/vm"), VMConfig.slugify("savedvm"))
    }

    func testNeverEmpty() {
        // All-symbol or empty input must never yield an empty slug — an empty slug
        // would collapse the per-VM library path (~/…/vms/<slug>/) and clobber data.
        XCTAssertEqual(VMConfig.slugify("!!!"), "vm")
        XCTAssertEqual(VMConfig.slugify(""), "vm")
        XCTAssertFalse(VMConfig.slugify("@#$%").isEmpty)
    }

    func testPersistedIDCannotEscapeLibraryPath() {
        let cfg = VMConfig(id: "../../victim/../outside", name: "Safe VM", displayName: "Safe VM",
                           backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: "", guestName: "", displayWidth: 0, displayHeight: 0)
        XCTAssertEqual(cfg.slug, "victim-outside")
        XCTAssertFalse(cfg.slug.contains("/"))
        XCTAssertFalse(cfg.slug.contains(".."))
    }
}

final class BackendKindTests: XCTestCase {
    func testLabels() {
        XCTAssertEqual(BackendKind.fastVZ.shortLabel, "Fast VZ")
        XCTAssertEqual(BackendKind.qemuCompat.shortLabel, "QEMU")
        XCTAssertEqual(BackendKind.fastVZ.detailLabel, "Fast (Apple VZ)")
        XCTAssertEqual(BackendKind.hvfEngine.detailLabel, "Native (HVF · Preview)")
        XCTAssertTrue(BackendKind.fastVZ.available)
        XCTAssertTrue(BackendKind.hvfEngine.available)
    }

    func testConfigEngineKindFallsBackToFastVZForUnknown() {
        var cfg = VMConfig(id: "x", name: "x", displayName: "x", backendKind: "bogus",
                           bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
                           handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                           guestName: "", displayWidth: 0, displayHeight: 0)
        XCTAssertEqual(cfg.engineKind, .fastVZ)
        XCTAssertEqual(cfg.engineShortLabel, "Fast VZ")
        cfg.backendKind = "qemu-compat"
        XCTAssertEqual(cfg.engineKind, .qemuCompat)
        XCTAssertEqual(cfg.engineDetailLabel, "Compatibility (QEMU)")
    }

    func testMakeBackendReturnsHvfWindowsBackend() {
        let cfg = VMConfig(id: "win", name: "Windows", displayName: "Windows VM", backendKind: "hvf-engine",
                           bootMode: nil, bundlePath: "/tmp/win.vmbridge", runnerPath: "", launchSpecPath: "",
                           handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                           guestName: "win", displayWidth: 1280, displayHeight: 800)
        XCTAssertTrue(cfg.makeBackend() is HvfWindowsBackend)
    }
}

final class VMLibraryPersistenceTests: XCTestCase {
    private func config(id: String = "saved-vm") -> VMConfig {
        VMConfig(id: id, name: "Saved VM", displayName: "Saved VM", backendKind: "fast-vz",
                 bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
                 handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                 guestName: "", displayWidth: 0, displayHeight: 0)
    }

    func testSaveWritesNormalizedIdentityAtomically() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertTrue(VMLibrary.save(config(id: "../saved/vm"), rootURL: root))
        let url = root.appendingPathComponent("saved-vm/vm.json")
        let decoded = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: url))
        XCTAssertEqual(decoded.id, "saved-vm")
    }

    func testSaveReportsUnwritableLibraryRoot() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("not a directory".utf8).write(to: root)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertFalse(VMLibrary.save(config(), rootURL: root))
    }

    func testDeleteReportsSuccessAndRemovesOnlyNormalizedChild() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let vm = root.appendingPathComponent("safe-vm")
        try FileManager.default.createDirectory(at: vm, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertTrue(VMLibrary.delete("../safe/vm", rootURL: root))
        XCTAssertFalse(FileManager.default.fileExists(atPath: vm.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.path))
    }

    func testDeleteReportsMissingLibraryEntry() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertFalse(VMLibrary.delete("missing", rootURL: root))
    }
}

final class ShellCommandSafetyTests: XCTestCase {
    func testShellCommandPreservesHostileArgumentsAsData() {
        let hostile = "quote' $(printf INJECTED) ; touch /tmp/never"
        let command = Shell.shellCommand("/usr/bin/printf", ["%s", hostile])
        let result = Shell.run("/bin/sh", ["-c", command])

        XCTAssertEqual(result.code, 0)
        XCTAssertEqual(result.output, hostile)
    }

    func testBackendLaunchCommandsQuoteImportedConfigValues() {
        let hostilePath = "/tmp/a'$(touch injected),disk"
        let config = VMConfig(id: "unsafe", name: "VM'; touch injected; '98", displayName: "Unsafe",
                              backendKind: "qemu-compat", bootMode: nil, bundlePath: hostilePath,
                              runnerPath: hostilePath + "/runner", launchSpecPath: "",
                              handoffPath: hostilePath + "/handoff.json", sshKeyPath: "", sshUser: "",
                              leasesPath: "", guestName: "", displayWidth: 1280, displayHeight: 800,
                              isoPath: hostilePath + "/installer.iso", diskPath: hostilePath + "/disk.qcow2",
                              memMiB: 4096, cpuCount: 4)

        let fast = FastVZBackend(config).launchCommand()
        let qemu = QemuCompatBackend(config).launchCommand()
        XCTAssertTrue(fast.contains(Shell.shQuote(config.runnerPath)))
        XCTAssertTrue(fast.contains(Shell.shQuote(config.handoffPath)))
        XCTAssertTrue(qemu.contains(Shell.shQuote(config.name)))
        XCTAssertTrue(qemu.contains("a'\\''$(touch injected),,disk"))
        XCTAssertFalse(qemu.contains("-name \(config.name)"))
    }

    func testFastBackendRejectsLeaseContentThatIsNotIPv4() throws {
        let leases = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: leases) }
        try Data("name=guest\nip_address=127.0.0.1;touch /tmp/injected\n".utf8).write(to: leases)
        let config = VMConfig(id: "safe", name: "Safe", displayName: "Safe", backendKind: "fast-vz",
                              bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
                              handoffPath: "", sshKeyPath: "", sshUser: "user", leasesPath: leases.path,
                              guestName: "guest", displayWidth: 1, displayHeight: 1)

        XCTAssertNil(FastVZBackend(config).currentIP())
    }

    func testFastBackendRejectsSSHOptionLikeUserBeforeConnecting() {
        let config = VMConfig(id: "safe", name: "Safe", displayName: "Safe", backendKind: "fast-vz",
                              bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
                              handoffPath: "", sshKeyPath: "", sshUser: "-oProxyCommand=bad", leasesPath: "",
                              guestName: "guest", displayWidth: 1, displayHeight: 1)
        let result = FastVZBackend(config).runInGuest("true")

        XCTAssertEqual(result.code, -1)
        XCTAssertTrue(result.output.contains("SSH 사용자 이름"))
    }

    func testShellRunDrainsButRetainsOnlyBoundedOutputTail() {
        let result = Shell.run(
            "/usr/bin/printf",
            [String(repeating: "x", count: 10_000)],
            outputLimitBytes: 256
        )

        XCTAssertEqual(result.code, 0)
        XCTAssertTrue(result.output.hasPrefix("[출력 일부 생략"))
        XCTAssertTrue(result.output.hasSuffix(String(repeating: "x", count: 256)))
        XCTAssertLessThan(result.output.utf8.count, 400)
    }

    func testShellRunClosesPipeInheritedByGrandchild() {
        let started = Date()
        let result = Shell.run("/bin/sh", ["-c", "(/bin/sleep 2) & /usr/bin/printf done"], timeout: 1)

        XCTAssertEqual(result.code, 0)
        XCTAssertEqual(result.output, "done")
        XCTAssertLessThan(Date().timeIntervalSince(started), 1.5)
    }
}

@MainActor
final class ControlModelLogBoundTests: XCTestCase {
    func testBoundedLogKeepsNewestContentAndMarksOmission() {
        let value = "old-" + String(repeating: "n", count: 100)
        let bounded = ControlModel.boundedLog(value, limit: 12)

        XCTAssertTrue(bounded.hasPrefix("… 이전 로그 생략 …\n"))
        XCTAssertTrue(bounded.hasSuffix(String(repeating: "n", count: 12)))
        XCTAssertFalse(bounded.contains("old-"))
    }
}
