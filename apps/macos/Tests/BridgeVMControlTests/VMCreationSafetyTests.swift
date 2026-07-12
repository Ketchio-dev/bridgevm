import XCTest
@testable import BridgeVMControl

final class VMCreationSafetyTests: XCTestCase {
    func testVMNameNormalizationTrimsAndRejectsUnsafeNames() {
        XCTAssertEqual(VMLibrary.normalizedVMName("  정상 VM  \n"), "정상 VM")
        XCTAssertNil(VMLibrary.normalizedVMName(" \n\t "))
        XCTAssertNil(VMLibrary.normalizedVMName("line\nbreak"))
        XCTAssertNil(VMLibrary.normalizedVMName("control\u{0000}character"))
        XCTAssertNil(VMLibrary.normalizedVMName(String(repeating: "a", count: VMLibrary.maximumVMNameCharacters + 1)))
        XCTAssertNil(VMLibrary.normalizedVMName(String(repeating: "한", count: 100)))
    }

    func testInvalidVMNameDoesNotReserveStorage() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let source = temp.appendingPathComponent("source.vmbridge", isDirectory: true)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)

        XCTAssertNil(VMLibrary.cloneUbuntu(
            name: "bad\nname",
            template: makeTemplate(bundlePath: source.path),
            storageDir: storage
        ))
        XCTAssertFalse(FileManager.default.fileExists(atPath: storage.path))
    }

    func testWindowsCreationCopiesISOIntoManagedBundle() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let sourceISO = temp.appendingPathComponent("Windows.iso")
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        let contents = Data("stable installer media".utf8)
        try contents.write(to: sourceISO)

        let config = try XCTUnwrap(VMLibrary.createWindows(
            name: "Self Contained Windows",
            isoPath: sourceISO.path,
            template: makeTemplate(bundlePath: temp.path),
            storageDir: storage,
            persist: false,
            diskCreator: makeFakeDisk
        ))

        let managedISO = try XCTUnwrap(config.isoPath)
        XCTAssertEqual(managedISO, config.bundlePath + "/disks/installer.iso")
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: managedISO)), contents)
        try FileManager.default.removeItem(at: sourceISO)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: managedISO)), contents)
    }

    func testWindowsCreationDereferencesSymlinkedISO() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let sourceISO = temp.appendingPathComponent("actual.iso")
        let linkedISO = temp.appendingPathComponent("selected.iso")
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try Data("installer".utf8).write(to: sourceISO)
        try FileManager.default.createSymbolicLink(at: linkedISO, withDestinationURL: sourceISO)

        let config = try XCTUnwrap(VMLibrary.createWindows(
            name: "Linked ISO",
            isoPath: linkedISO.path,
            template: makeTemplate(bundlePath: temp.path),
            storageDir: storage,
            persist: false,
            diskCreator: makeFakeDisk
        ))

        let managedISO = try XCTUnwrap(config.isoPath)
        let values = try URL(fileURLWithPath: managedISO).resourceValues(forKeys: [.isSymbolicLinkKey])
        XCTAssertFalse(values.isSymbolicLink == true)
        XCTAssertEqual(try String(contentsOfFile: managedISO), "installer")
    }

    func testWindowsDiskCreationFailureRemovesCopiedISOAndReservation() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let sourceISO = temp.appendingPathComponent("Windows.iso")
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try Data("installer".utf8).write(to: sourceISO)

        let result = VMLibrary.createWindows(
            name: "Failed Windows",
            isoPath: sourceISO.path,
            template: makeTemplate(bundlePath: temp.path),
            storageDir: storage,
            persist: false,
            diskCreator: { _, _ in false }
        )

        XCTAssertNil(result)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.path), [])
        XCTAssertEqual(try String(contentsOf: sourceISO), "installer")
    }

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

    private func makeFakeDisk(at path: String, sizeGiB: Int) -> Bool {
        FileManager.default.createFile(atPath: path, contents: Data("qcow2-\(sizeGiB)".utf8))
    }

    private func makeTempDir() throws -> URL {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}
