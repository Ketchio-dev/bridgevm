import XCTest
@testable import BridgeVMControl

/// The irreversible-delete dialog must name the bundle that deletion actually
/// removes. Deleting a library VM recursively removes `<root>/<slug>/`, so a
/// managed bundle behind a stale saved absolute path is the entry's
/// `bundle.vmbridge`, never the stale path a user would check and find empty.
final class LibraryDeletionPromptTests: XCTestCase {
    private let engines = [("fast-vz", "fast"), ("hvf-engine", "win")]

    private func scratch() -> (temp: URL, root: URL) {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        return (temp, temp.appendingPathComponent("library", isDirectory: true))
    }

    private func config(_ slug: String, engine: String, bundlePath: String) -> VMConfig {
        var cfg = VMConfig(id: slug, name: slug, displayName: slug, backendKind: engine, bootMode: nil,
                           bundlePath: bundlePath, runnerPath: "", launchSpecPath: "", handoffPath: "",
                           sshKeyPath: "", sshUser: "", leasesPath: "", guestName: slug,
                           displayWidth: 1, displayHeight: 1)
        cfg.installPending = false
        return cfg
    }

    private func assertManaged(_ cfg: VMConfig, root: URL, names path: String, _ engine: String) {
        XCTAssertEqual(VMLibrary.deletionImpact(for: cfg, rootURL: root), .managedBundleDeleted, engine)
        XCTAssertEqual(VMLibrary.deletionTarget(for: cfg, rootURL: root)?.path, path, engine)
        let prompt = LibraryDeletionPrompt(config: cfg, rootURL: root)
        XCTAssertEqual(prompt.confirmTitle, "Delete \(cfg.name) and Its Disk", engine)
        XCTAssertEqual(prompt.message,
                       "This permanently deletes the VM settings and the virtual disk at \(path).", engine)
    }

    func testStaleSavedPathNamesTheEntryBundleThatDeletionRemoves() throws {
        let (temp, root) = scratch()
        defer { try? FileManager.default.removeItem(at: temp) }
        for (engine, slug) in engines {
            let entryBundle = root.appendingPathComponent("\(slug)/bundle.vmbridge", isDirectory: true)
            try FileManager.default.createDirectory(at: entryBundle, withIntermediateDirectories: true)
            let stale = temp.appendingPathComponent("old-home/library/\(slug)/bundle.vmbridge").path
            let cfg = config(slug, engine: engine, bundlePath: stale)
            XCTAssertTrue(VMLibrary.save(cfg, rootURL: root))
            assertManaged(cfg, root: root, names: entryBundle.path, engine)
            XCTAssertFalse(LibraryDeletionPrompt(config: cfg, rootURL: root).message.contains(stale), engine)
        }
    }

    func testManagedBundleInsideTheEntryKeepsNamingItsSavedPath() throws {
        let (temp, root) = scratch()
        defer { try? FileManager.default.removeItem(at: temp) }
        for (engine, slug) in engines {
            let bundle = root.appendingPathComponent("\(slug)/bundle.vmbridge", isDirectory: true)
            try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
            let cfg = config(slug, engine: engine, bundlePath: bundle.path)
            XCTAssertTrue(VMLibrary.save(cfg, rootURL: root))
            assertManaged(cfg, root: root, names: bundle.path, engine)
        }
    }

    func testExternalRegistrationNamesNoTargetAndTheKeptBundle() throws {
        let (temp, root) = scratch()
        defer { try? FileManager.default.removeItem(at: temp) }
        for (engine, slug) in engines {
            let external = temp.appendingPathComponent("external/\(slug)/bundle.vmbridge", isDirectory: true)
            try FileManager.default.createDirectory(at: external, withIntermediateDirectories: true)
            let cfg = config(slug, engine: engine, bundlePath: external.path)
            XCTAssertTrue(VMLibrary.save(cfg, rootURL: root))
            XCTAssertEqual(VMLibrary.deletionImpact(for: cfg, rootURL: root), .registrationOnly, engine)
            XCTAssertNil(VMLibrary.deletionTarget(for: cfg, rootURL: root), engine)
            let prompt = LibraryDeletionPrompt(config: cfg, rootURL: root)
            XCTAssertEqual(prompt.confirmTitle, "Remove \(slug) from Library", engine)
            XCTAssertEqual(prompt.message,
                           "This removes the VM from your library. Its files stay in \(external.path).", engine)
        }
    }

    /// The path stays so people can find the files; under the home folder it starts with ~.
    func testAPathInTheHomeFolderIsShownWithATilde() {
        let (_, root) = scratch()
        let bundle = NSHomeDirectory() + "/BridgeVM External/win/bundle.vmbridge"
        let prompt = LibraryDeletionPrompt(config: config("win", engine: "hvf-engine", bundlePath: bundle), rootURL: root)
        XCTAssertEqual(prompt.message,
                       "This removes the VM from your library. Its files stay in ~/BridgeVM External/win/bundle.vmbridge.")
    }

    /// Fast VZ deletes the entry directly, so the named target can be checked
    /// against what the real deletion leaves behind.
    func testFastVZDeletionRemovesTheNamedTargetAndKeepsExternalBundles() throws {
        let (temp, root) = scratch()
        defer { try? FileManager.default.removeItem(at: temp) }
        let entryBundle = root.appendingPathComponent("stale/bundle.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: entryBundle, withIntermediateDirectories: true)
        try Data("disk".utf8).write(to: entryBundle.appendingPathComponent("disk"))
        let stale = config("stale", engine: "fast-vz", bundlePath: temp.appendingPathComponent("gone/bundle.vmbridge").path)
        let external = temp.appendingPathComponent("external/ext/bundle.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: external, withIntermediateDirectories: true)
        let ext = config("ext", engine: "fast-vz", bundlePath: external.path)
        XCTAssertTrue(VMLibrary.save(stale, rootURL: root) && VMLibrary.save(ext, rootURL: root))
        let target = try XCTUnwrap(VMLibrary.deletionTarget(for: stale, rootURL: root))
        XCTAssertTrue(FileManager.default.fileExists(atPath: target.path))
        XCTAssertTrue(LibraryDeletionProtection.delete(stale, rootURL: root))
        XCTAssertFalse(FileManager.default.fileExists(atPath: target.path))
        XCTAssertTrue(LibraryDeletionProtection.delete(ext, rootURL: root))
        XCTAssertTrue(FileManager.default.fileExists(atPath: external.path))
    }
}
