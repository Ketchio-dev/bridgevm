import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

final class VMRelocationRecordReaderTests: XCTestCase {
    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }

    func testRegularRecordAndOversizedSparseFile() throws {
        let root = try directory()
        let file = root.appendingPathComponent("record")
        let payload = Data("private-record".utf8)
        try payload.write(to: file)
        XCTAssertEqual(VMRelocationRecordReader.read(file), payload)
        let handle = try FileHandle(forWritingTo: file)
        try handle.truncate(atOffset: 128 * 1024 * 1024)
        try handle.close()
        XCTAssertNil(VMRelocationRecordReader.read(file))
    }

    func testSymlinkDirectoryMissingFileAndFIFOAreRefused() throws {
        let root = try directory()
        let file = root.appendingPathComponent("record")
        try Data("private-record".utf8).write(to: file)
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: file)
        XCTAssertNil(VMRelocationRecordReader.read(alias))
        XCTAssertNil(VMRelocationRecordReader.read(root))
        XCTAssertNil(VMRelocationRecordReader.read(root.appendingPathComponent("missing")))
        let fifo = root.appendingPathComponent("fifo")
        XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
        XCTAssertNil(VMRelocationRecordReader.read(fifo))
    }
}
