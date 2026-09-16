import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStatusConfigStabilityTests: XCTestCase {
    func testInPlaceWriteAndAtomicReplacementInvalidateCapturedConfiguration() throws {
        for replacement in [false, true] {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("status-stability-\(UUID().uuidString)")
            let entry = root.appendingPathComponent("vm"), path = entry.appendingPathComponent("vm.json")
            try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            try Data("initial".utf8).write(to: path)
            let library = open(root.path, O_RDONLY | O_DIRECTORY), directory = open(entry.path, O_RDONLY | O_DIRECTORY)
            let file = open(path.path, O_RDONLY)
            XCTAssertTrue(library >= 0 && directory >= 0 && file >= 0)
            defer { close(file); close(directory); close(library) }
            var before = stat()
            XCTAssertEqual(fstat(file, &before), 0)
            try NativeCLIStatusConfigReader.validateUnchanged(file: file, directory: directory, library: library, id: "vm", before: before)
            try Data("updated configuration".utf8).write(to: path, options: replacement ? .atomic : [])
            XCTAssertThrowsError(try NativeCLIStatusConfigReader.validateUnchanged(file: file, directory: directory,
                library: library, id: "vm", before: before))
        }
    }

    func testReplacedRegistrationDirectoryInvalidatesCapturedConfiguration() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("status-stability-\(UUID().uuidString)")
        let entry = root.appendingPathComponent("vm"), path = entry.appendingPathComponent("vm.json")
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("initial".utf8).write(to: path)
        let library = open(root.path, O_RDONLY | O_DIRECTORY), directory = open(entry.path, O_RDONLY | O_DIRECTORY)
        let file = open(path.path, O_RDONLY)
        defer { close(file); close(directory); close(library) }
        var before = stat()
        XCTAssertEqual(fstat(file, &before), 0)
        try FileManager.default.moveItem(at: entry, to: root.appendingPathComponent("old-vm"))
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        XCTAssertThrowsError(try NativeCLIStatusConfigReader.validateUnchanged(file: file, directory: directory,
            library: library, id: "vm", before: before))
    }
}
