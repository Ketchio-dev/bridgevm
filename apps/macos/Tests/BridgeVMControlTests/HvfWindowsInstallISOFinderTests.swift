import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallISOFinderTests: XCTestCase {
    func testSelectsTheISOOrItsExistingImmediateDirectory() throws {
        typealias Target = HvfWindowsInstallISOButton.FinderTarget
        let manager = FileManager.default
        let root = manager.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try manager.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? manager.removeItem(at: root) }
        let folder = root.appendingPathComponent("disks", isDirectory: true)
        let iso = folder.appendingPathComponent("installer.iso")

        XCTAssertNil(Target(path: ""))
        XCTAssertNil(Target(path: "disks/installer.iso"))
        XCTAssertNil(Target(path: iso.path), "the existing grandparent is unrelated")
        try manager.createDirectory(at: folder, withIntermediateDirectories: true)
        XCTAssertEqual(Target(path: iso.path), .folder(folder))
        try Data([0x42]).write(to: iso)
        XCTAssertEqual(Target(path: iso.path), .file(iso))
        try manager.removeItem(at: iso)
        XCTAssertEqual(Target(path: iso.path), .folder(folder))
    }
}
