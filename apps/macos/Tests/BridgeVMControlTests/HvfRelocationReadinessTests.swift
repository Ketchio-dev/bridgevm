import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfRelocationReadinessTests: XCTestCase {
    func testCachedConfigurationIsNotLaunchReadyWithPendingRelocation() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? fm.removeItem(at: root) }
        let library = root.appendingPathComponent("library")
        let bundle = library.appendingPathComponent("cached-fixture/bundle.vmbridge")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent()] {
            try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data("private-disk".utf8).write(to: disk)
        try Data().write(to: vars)
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: 64 * 1024 * 1024)
        try handle.close()
        let config = VMConfig(id: "cached-fixture", name: "Cached Fixture",
            displayName: "Cached Fixture", backendKind: "hvf-engine",
            bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "Windows", displayWidth: 800, displayHeight: 600)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        let repo = root.appendingPathComponent("repo")
        for relative in ["scripts/run-hvf-windows-installed-boot.sh", "target/release/examples/hvf_gic_boot_probe"] {
            let executable = repo.appendingPathComponent(relative)
            try fm.createDirectory(at: executable.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("#!/bin/sh\nexit 0\n".utf8).write(to: executable)
            try fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        }
        var cached = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: library))
        cached.vtpmStateDir = nil
        cached.vtpmKeyID = nil
        let before = cached.readiness(repoRoot: repo)
        XCTAssertTrue(before.launchReady, "\(before.launchBlockers)")
        guard before.launchReady else { return }
        var moved = config
        moved.bundlePath = root.appendingPathComponent("target/bundle.vmbridge").path
        _ = try VMRelocationJournal.begin(original: config, moved: moved, rootURL: library)
        XCTAssertTrue(VMLibrary.list(rootURL: library).isEmpty)
        XCTAssertTrue(cached.readiness(repoRoot: repo).launchBlockers.contains { $0.code == "relocation-pending" },
                       "A cached configuration must not bypass an unresolved relocation")
    }
}
