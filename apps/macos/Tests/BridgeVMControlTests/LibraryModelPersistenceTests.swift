import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelPersistenceTests: XCTestCase {
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

    private func makeTempDir() throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
