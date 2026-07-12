import XCTest
@testable import BridgeVMControl

final class VMCreationSafetyTests: XCTestCase {
    func testCloneRejectsStorageInsideSourceBeforeCreatingAnything() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("source.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        try Data("keep".utf8).write(to: source.appendingPathComponent("user-data"))
        let nestedStorage = source.appendingPathComponent("nested/library", isDirectory: true)

        let result = VMLibrary.cloneUbuntu(
            name: "Recursive Clone",
            template: makeTemplate(bundlePath: source.path),
            storageDir: nestedStorage
        )

        XCTAssertNil(result)
        XCTAssertFalse(FileManager.default.fileExists(atPath: nestedStorage.path))
        XCTAssertEqual(try String(contentsOf: source.appendingPathComponent("user-data")), "keep")
    }

    func testCloneRejectsSymlinkedStorageThatResolvesInsideSource() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("source.vmbridge", isDirectory: true)
        let nested = source.appendingPathComponent("nested", isDirectory: true)
        let storageLink = temp.appendingPathComponent("storage-link")
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: storageLink, withDestinationURL: nested)

        let result = VMLibrary.cloneUbuntu(
            name: "Linked Recursive Clone",
            template: makeTemplate(bundlePath: source.path),
            storageDir: storageLink
        )

        XCTAssertNil(result)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: nested.path), [])
    }

    func testCloneRejectsInvalidDimensionsBeforeReservingDestination() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("source.vmbridge", isDirectory: true)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)

        XCTAssertNil(VMLibrary.cloneUbuntu(
            name: "Invalid Size",
            template: makeTemplate(bundlePath: source.path),
            storageDir: storage,
            width: 0,
            height: 900
        ))
        XCTAssertFalse(FileManager.default.fileExists(atPath: storage.path))
    }

    func testPathContainmentUsesComponentsInsteadOfStringPrefixes() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("vm", isDirectory: true)
        let sibling = temp.appendingPathComponent("vm-backups", isDirectory: true)

        XCTAssertTrue(VMLibrary.isSameOrDescendant(source.appendingPathComponent("child"), of: source))
        XCTAssertTrue(VMLibrary.isSameOrDescendant(source, of: source))
        XCTAssertFalse(VMLibrary.isSameOrDescendant(sibling, of: source))
    }

    private func makeTemplate(bundlePath: String) -> VMConfig {
        VMConfig(id: "template", name: "Template", displayName: "Template",
                 backendKind: "fast-vz", bootMode: "direct-kernel", bundlePath: bundlePath,
                 runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
                 sshUser: "", leasesPath: "", guestName: "template",
                 displayWidth: 1440, displayHeight: 900)
    }

    private func makeTempDir() throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
