import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRelocationRecordWriterTests: XCTestCase {
    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    func testPrivateRecordCreationAndExistingRecordRefusal() throws {
        let root = try directory()
        let file = root.appendingPathComponent("record")
        let original = Data("original".utf8)
        try VMRelocationRecordWriter.write(original, to: file)
        XCTAssertEqual(VMRelocationRecordReader.read(file), original)
        let attributes = try FileManager.default.attributesOfItem(atPath: file.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        XCTAssertThrowsError(try VMRelocationRecordWriter.write(Data("replacement".utf8), to: file))
        XCTAssertEqual(try Data(contentsOf: file), original)
    }

    func testSymlinkCannotReplaceItsTarget() throws {
        let root = try directory()
        let target = root.appendingPathComponent("target")
        let original = Data("untouched".utf8)
        try original.write(to: target)
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        XCTAssertThrowsError(try VMRelocationRecordWriter.write(Data("replacement".utf8), to: alias))
        XCTAssertEqual(try Data(contentsOf: target), original)
    }
}
