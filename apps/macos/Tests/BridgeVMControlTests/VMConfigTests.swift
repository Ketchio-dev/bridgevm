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
