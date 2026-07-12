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
