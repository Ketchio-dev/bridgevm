import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDefaultShareCollisionTests: XCTestCase {
    private func make(_ name: String, root: URL) throws -> VMConfig {
        let iso = root.appendingPathComponent("test.iso")
        if !FileManager.default.fileExists(atPath: iso.path) { try Data([1]).write(to: iso) }
        guard case let .created(config)? = VMLibrary.createWindowsHVFInstall(name: name,
            isoPath: iso.path, diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            libraryRoot: root) else { throw NSError(domain: "creation", code: 1) }
        return try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: root.appendingPathComponent(config.slug + "/vm.json")))
    }
    private func key(_ path: String?) -> String { (path ?? "").precomposedStringWithCanonicalMapping.lowercased() }
    private func root() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("share-collision-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }
    func testThreeVMNamesNeverSelectTheSameFolder() throws {
        let root = try root(); defer { try? FileManager.default.removeItem(at: root) }
        let first = try make("Windows", root: root)
        let second = try make("Windows", root: root)
        let third = try make("Windows (windows-2)", root: root)
        XCTAssertEqual(Set([first, second, third].map { key($0.sharedFolderHostPath) }).count, 3,
            "Three unrelated registrations must not sync the same folder")
    }
    func testCaseInsensitiveNamesNeverSelectTheSameFolder() throws {
        let root = try root(); defer { try? FileManager.default.removeItem(at: root) }
        let first = try make("Windows", root: root)
        var second = first
        second.name = "windows"
        second.id = VMConfig.slugify(second.name)
        second.enableNewWindowsShare(otherRegistrations: [first])
        XCTAssertNotEqual(key(first.sharedFolderHostPath), key(second.sharedFolderHostPath),
            "Case-insensitive Mac volumes alias these names")
    }
    func testUnicodeEquivalentNamesNeverSelectTheSameFolder() throws {
        let root = try root(); defer { try? FileManager.default.removeItem(at: root) }
        let first = try make("Café", root: root)
        var second = first; second.name = "Cafe\u0301"; second.id = VMConfig.slugify(second.name)
        second.enableNewWindowsShare(otherRegistrations: [first])
        XCTAssertNotEqual(key(first.sharedFolderHostPath), key(second.sharedFolderHostPath))
    }
    func testDeletedVMFolderNameIsFreeAgain() throws {
        let root = try root(); defer { try? FileManager.default.removeItem(at: root) }
        let first = try make("Windows", root: root)
        let second = try make("Windows", root: root)
        XCTAssertEqual((try XCTUnwrap(second.sharedFolderHostPath) as NSString).lastPathComponent, "Windows 2")
        try FileManager.default.removeItem(at: root.appendingPathComponent(second.slug))
        let replacement = try make("Windows", root: root)
        XCTAssertEqual(replacement.sharedFolderHostPath, second.sharedFolderHostPath)
        XCTAssertNotEqual(replacement.sharedFolderHostPath, first.sharedFolderHostPath)
    }
}
