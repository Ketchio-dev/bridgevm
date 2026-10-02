import Foundation
import XCTest
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif
@testable import BridgeVMControl

final class HvfTailRotationTests: XCTestCase {
    func testLargerReplacementDoesNotJoinStalePartialOrSkipNewPrefix() throws {
        try withRoot { root in
            let file = root.appendingPathComponent("run.log")
            try Data("old-partial".utf8).write(to: file)
            let reader = TailOffsetReader()
            XCTAssertEqual(reader.readNewLines(from: file), [])
            try replace(file, with: "new-line-one\nnew-line-two\n")
            XCTAssertEqual(reader.readNewLines(from: file), ["new-line-one", "new-line-two"])
        }
    }

    func testEqualSizedReplacementIsNotMistakenForNoGrowth() throws {
        try withRoot { root in
            let file = root.appendingPathComponent("run.log")
            try Data("old\n".utf8).write(to: file)
            let reader = TailOffsetReader()
            XCTAssertEqual(reader.readNewLines(from: file), ["old"])
            try replace(file, with: "new\n")
            XCTAssertEqual(reader.readNewLines(from: file), ["new"])
        }
    }

    func testChangingPathsResetsEvenForAnotherNameOfSameInode() throws {
        try withRoot { root in
            let first = root.appendingPathComponent("first"), second = root.appendingPathComponent("second")
            try Data("first\n".utf8).write(to: first)
            try FileManager.default.linkItem(at: first, to: second)
            let reader = TailOffsetReader()
            XCTAssertEqual(reader.readNewLines(from: first), ["first"])
            XCTAssertEqual(reader.readNewLines(from: second), ["first"])
        }
    }

    func testRotationClearsOversizedRecordDiscardState() throws {
        try withRoot { root in
            let file = root.appendingPathComponent("run.log")
            try Data(repeating: 120, count: 3 * 1_048_576).write(to: file)
            let reader = TailOffsetReader()
            for _ in 0..<3 { XCTAssertEqual(reader.readNewLines(from: file), []) }
            try replace(file, with: "fresh\n")
            XCTAssertEqual(reader.readNewLines(from: file), ["fresh"])
        }
    }

    func testFIFOIsOpenedWithoutBlockingAndRefusedAsNonregular() throws {
        try withRoot { root in
            let fifo = root.appendingPathComponent("fifo")
            XCTAssertEqual(mkfifo(fifo.path, 0o600), 0)
            let handle = try XCTUnwrap(HvfTailFileCursor.openFile(fifo))
            defer { try? handle.close() }
            var cursor = HvfTailFileCursor(startingAt: 0)
            XCTAssertNil(cursor.admit(handle, path: fifo.path))
            XCTAssertEqual(TailOffsetReader().readNewLines(from: fifo), [])
        }
    }

    private func replace(_ file: URL, with text: String) throws {
        let next = file.deletingLastPathComponent().appendingPathComponent(UUID().uuidString)
        try Data(text.utf8).write(to: next)
        try FileManager.default.removeItem(at: file)
        try FileManager.default.moveItem(at: next, to: file)
    }

    private func withRoot(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root)
    }
}
