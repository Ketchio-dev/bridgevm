import XCTest
@testable import BridgeVMControl

final class LibraryImportedDeletionTargetTests: XCTestCase {
    private struct Fixture {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        var root: URL { temp.appendingPathComponent("library") }
        var entry: URL { root.appendingPathComponent("imported") }
        var bundle: URL { entry.appendingPathComponent("bundle") }
        var layout: FirstRunImport.BundleLayout { .init(bundleURL: bundle) }

        func config(bundle: URL, engine: String = "hvf-engine") -> VMConfig {
            var cfg = VMConfig(id: "imported", name: "Imported", displayName: "Imported", backendKind: engine,
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Imported",
                displayWidth: 1, displayHeight: 1)
            cfg.diskPath = FirstRunImport.BundleLayout(bundleURL: bundle).diskURL.path
            cfg.installPending = false
            return cfg
        }

        func makeBundle() throws {
            try FileManager.default.createDirectory(at: layout.diskURL.deletingLastPathComponent(),
                                                    withIntermediateDirectories: true)
            try Data("synthetic disk".utf8).write(to: layout.diskURL)
        }
        func remove() { try? FileManager.default.removeItem(at: temp) }
    }

    func testRelocatedImportedBundleNamesTheActualManagedDirectory() throws {
        let f = Fixture(); defer { f.remove() }
        try f.makeBundle()
        let cfg = f.config(bundle: f.temp.appendingPathComponent("old-library/imported/bundle"))
        XCTAssertTrue(VMLibrary.save(cfg, rootURL: f.root))
        XCTAssertEqual(VMLibrary.deletionTarget(for: cfg, rootURL: f.root), f.bundle)
        XCTAssertEqual(VMLibrary.deletionImpact(for: cfg, rootURL: f.root), .managedBundleDeleted)
        let prompt = LibraryDeletionPrompt(config: cfg, rootURL: f.root)
        XCTAssertTrue(prompt.message.contains(f.bundle.path))
        XCTAssertFalse(prompt.message.contains(cfg.bundlePath))
        XCTAssertEqual(prompt.confirmTitle, "Imported 및 가상 디스크 삭제")
        // The registration-only escape hatch must still refuse this owned disk.
        XCTAssertNotEqual(HvfUnavailableMediaDeletion.delete(cfg, rootURL: f.root), true)
        XCTAssertEqual(try Data(contentsOf: f.layout.diskURL), Data("synthetic disk".utf8))
    }

    func testDetachedExternalImportStillRemovesOnlyTheRegistration() throws {
        let f = Fixture(); defer { f.remove() }
        let cfg = f.config(bundle: f.temp.appendingPathComponent("detached/imported/bundle"))
        XCTAssertTrue(VMLibrary.save(cfg, rootURL: f.root))
        XCTAssertNil(VMLibrary.deletionTarget(for: cfg, rootURL: f.root))
        XCTAssertEqual(VMLibrary.deletionImpact(for: cfg, rootURL: f.root), .registrationOnly)
        XCTAssertEqual(HvfUnavailableMediaDeletion.delete(cfg, rootURL: f.root), true)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.entry.path))
    }

    func testManagedNameSymlinksDoNotClaimTheExternalTargetIsDeleted() throws {
        for name in ["bundle", "bundle.vmbridge"] {
            let f = Fixture(); defer { f.remove() }
            let external = f.temp.appendingPathComponent("external")
            try FileManager.default.createDirectory(at: external, withIntermediateDirectories: true)
            let sentinel = external.appendingPathComponent("disk")
            try Data("keep".utf8).write(to: sentinel)
            let cfg = f.config(bundle: external, engine: "fast-vz")
            XCTAssertTrue(VMLibrary.save(cfg, rootURL: f.root))
            try FileManager.default.createSymbolicLink(at: f.entry.appendingPathComponent(name),
                                                       withDestinationURL: external)
            XCTAssertNil(VMLibrary.deletionTarget(for: cfg, rootURL: f.root), name)
            XCTAssertEqual(VMLibrary.deletionImpact(for: cfg, rootURL: f.root), .registrationOnly, name)
            XCTAssertTrue(LibraryDeletionProtection.delete(cfg, rootURL: f.root))
            XCTAssertEqual(try Data(contentsOf: sentinel), Data("keep".utf8), name)
        }
    }

    func testManagedNameRegularFileIsNotReportedAsABundle() throws {
        let f = Fixture(); defer { f.remove() }
        let cfg = f.config(bundle: f.temp.appendingPathComponent("external"))
        XCTAssertTrue(VMLibrary.save(cfg, rootURL: f.root))
        try Data("file".utf8).write(to: f.bundle)
        XCTAssertNil(VMLibrary.deletionTarget(for: cfg, rootURL: f.root))
    }
}
