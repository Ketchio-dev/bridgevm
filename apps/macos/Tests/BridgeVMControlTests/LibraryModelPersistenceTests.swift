import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelPersistenceTests: XCTestCase {
    func testReloadReplacesIdleModelWhenPersistedConfigChanges() throws {
        let root = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: root) }
        let original = makeConfig()
        XCTAssertTrue(VMLibrary.save(original, rootURL: root))
        let library = makeLibrary(root: root)
        let first = library.model(for: try XCTUnwrap(library.vms.first))
        var changed = original
        changed.backendKind = "qemu-compat"
        changed.bundlePath = "/tmp/changed.vmbridge"
        XCTAssertTrue(VMLibrary.save(changed, rootURL: root))

        library.reload()
        let second = library.model(for: try XCTUnwrap(library.vms.first))

        XCTAssertFalse(first === second)
        XCTAssertEqual(second.config, changed)
        XCTAssertEqual(second.backend.kind, "qemu-compat")
    }

    func testReloadPreservesChangedModelUntilActiveVMBecomesIdle() throws {
        let root = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: root) }
        let original = makeConfig()
        XCTAssertTrue(VMLibrary.save(original, rootURL: root))
        let library = makeLibrary(root: root)
        let active = library.model(for: try XCTUnwrap(library.vms.first))
        active.running = true
        var changed = original
        changed.bundlePath = "/tmp/new-target.vmbridge"
        XCTAssertTrue(VMLibrary.save(changed, rootURL: root))

        library.reload()
        XCTAssertTrue(library.model(for: try XCTUnwrap(library.vms.first)) === active)

        active.running = false
        library.reload()
        XCTAssertFalse(library.model(for: try XCTUnwrap(library.vms.first)) === active)
    }

    func testReloadKeepsUnchangedModelIdentity() throws {
        let root = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertTrue(VMLibrary.save(makeConfig(), rootURL: root))
        let library = makeLibrary(root: root)
        let first = library.model(for: try XCTUnwrap(library.vms.first))

        library.reload()

        XCTAssertTrue(library.model(for: try XCTUnwrap(library.vms.first)) === first)
    }

    func testAddAdoptsPersistedConfigWithoutWritingAgain() throws {
        let root = try makeTempDir()
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: root.path)
            try? FileManager.default.removeItem(at: root)
        }
        let config = makeConfig()
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let entry = root.appendingPathComponent(config.slug, isDirectory: true)
        let configFile = entry.appendingPathComponent("vm.json")
        let original = try Data(contentsOf: configFile)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: entry.path)
        let model = LibraryModel(rootURL: root, migrateLegacy: false)

        XCTAssertTrue(model.add(config))

        XCTAssertEqual(model.selectedID, config.slug)
        XCTAssertEqual(model.vms.map(\.slug), [config.slug])
        XCTAssertEqual(try Data(contentsOf: configFile), original)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: entry.path)
    }

    func testAddRejectsConfigThatWasNotPersisted() throws {
        let root = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: root) }
        let model = LibraryModel(rootURL: root, migrateLegacy: false)

        XCTAssertFalse(model.add(makeConfig()))
        XCTAssertNil(model.selectedID)
        XCTAssertTrue(model.vms.isEmpty)
    }

    private func makeConfig() -> VMConfig {
        VMConfig(id: "persisted", name: "Persisted", displayName: "Persisted",
                 backendKind: "fast-vz", bootMode: "direct-kernel",
                 bundlePath: "/tmp/persisted.vmbridge", runnerPath: "",
                 launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                 leasesPath: "", guestName: "persisted", displayWidth: 1440, displayHeight: 900)
    }

    private func makeLibrary(root: URL) -> LibraryModel {
        LibraryModel(
            rootURL: root,
            migrateLegacy: false,
            modelFactory: { ControlModel(config: $0, startsAutomatically: false) }
        )
    }

    private func makeTempDir() throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
