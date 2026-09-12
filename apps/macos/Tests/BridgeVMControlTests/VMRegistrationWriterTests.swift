import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRegistrationWriterTests: XCTestCase {
    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    func testCreatesAndReplacesRegistrationWithPrivatePermissions() throws {
        let root = try directory()
        let file = root.appendingPathComponent("vm.json")
        try VMRegistrationWriter.write(Data("first".utf8), to: file)
        try VMRegistrationWriter.write(Data("second".utf8), to: file)
        XCTAssertEqual(try Data(contentsOf: file), Data("second".utf8))
        let attributes = try FileManager.default.attributesOfItem(atPath: file.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["vm.json"])
    }

    func testDirectoryDestinationIsNotRemoved() throws {
        let root = try directory()
        let destination = root.appendingPathComponent("vm.json")
        try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
        let marker = destination.appendingPathComponent("marker")
        try Data("retained".utf8).write(to: marker)
        XCTAssertThrowsError(try VMRegistrationWriter.write(Data("replacement".utf8), to: destination))
        XCTAssertEqual(try Data(contentsOf: marker), Data("retained".utf8))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["vm.json"])
    }
}
