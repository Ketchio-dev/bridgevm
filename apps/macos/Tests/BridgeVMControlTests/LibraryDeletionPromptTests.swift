import XCTest
@testable import BridgeVMControl

/// The irreversible-delete dialog must name the bundle that deletion actually
/// removes. Deleting a library VM recursively removes `<root>/<slug>/`, so a
/// managed bundle behind a stale saved absolute path is the entry's
/// `bundle.vmbridge`, never the stale path a user would check and find empty.
final class LibraryDeletionPromptTests: XCTestCase {
    private let guidance = "실행 중인 VM은 삭제할 수 없습니다. 제어 화면에서 완전히 중지한 뒤 진행하세요."
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
        XCTAssertEqual(prompt.confirmTitle, "\(cfg.name) 및 가상 디스크 삭제", engine)
        XCTAssertEqual(prompt.message,
                       "\(guidance) VM 설정과 \(path)에 있는 가상 디스크 데이터가 모두 영구 삭제되며 되돌릴 수 없습니다.", engine)
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
            XCTAssertEqual(prompt.confirmTitle, "\(slug) 등록 제거", engine)
            XCTAssertEqual(prompt.message,
                           "\(guidance) BridgeVM 라이브러리 등록만 제거하며 외부 번들 \(external.path)의 가상 디스크 데이터는 유지됩니다.", engine)
        }
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
