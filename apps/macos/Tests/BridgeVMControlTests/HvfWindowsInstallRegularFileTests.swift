import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallRegularFileTests: XCTestCase {
    func testReadsExactLimitAndUnboundedRegularFile() throws {
        try fixture { root in
            let file = root.appendingPathComponent("metadata")
            try Data("abc".utf8).write(to: file)
            XCTAssertEqual(try HvfWindowsInstallDurability.readRegularFile(file, maximumBytes: 3), Data("abc".utf8))
            XCTAssertEqual(try HvfWindowsInstallDurability.readRegularFile(file), Data("abc".utf8))
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(file, maximumBytes: 2))
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(file, maximumBytes: -1))
        }
    }

    func testZeroLimitAcceptsOnlyEmptyFile() throws {
        try fixture { root in
            let file = root.appendingPathComponent("empty")
            try Data().write(to: file)
            XCTAssertEqual(try HvfWindowsInstallDurability.readRegularFile(file, maximumBytes: 0), Data())
            try Data([1]).write(to: file)
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(file, maximumBytes: 0))
        }
    }

    func testRejectsSymlinksDirectoriesAndFIFOs() throws {
        try fixture { root in
            let file = root.appendingPathComponent("metadata")
            let link = root.appendingPathComponent("link")
            let fifo = root.appendingPathComponent("fifo")
            try Data("abc".utf8).write(to: file)
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: file)
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(link))
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(root))
            XCTAssertEqual(mkfifo(fifo.path, S_IRUSR | S_IWUSR), 0)
            XCTAssertThrowsError(try HvfWindowsInstallDurability.readRegularFile(fifo))
        }
    }

    func testGrowthBeyondLimitIsRejectedInsteadOfTruncated() {
        var calls = [Int]()
        XCTAssertThrowsError(try HvfWindowsInstallRegularFile.collect(maximumBytes: 3) { count in
            calls.append(count)
            return calls.count == 1 ? Data("abc".utf8) : Data("d".utf8)
        })
        XCTAssertEqual(calls, [4, 1])
    }

    func testExactLimitRequiresEndOfFileProbe() throws {
        var calls = [Int]()
        let data = try HvfWindowsInstallRegularFile.collect(maximumBytes: 3) { count in
            calls.append(count)
            return calls.count == 1 ? Data("abc".utf8) : Data()
        }
        XCTAssertEqual(data, Data("abc".utf8))
        XCTAssertEqual(calls, [4, 1])
    }

    func testLargestLimitDoesNotOverflowOrRequestUnboundedRead() throws {
        var requested = 0
        let data = try HvfWindowsInstallRegularFile.collect(maximumBytes: Int.max) { count in
            requested = count
            return Data()
        }
        XCTAssertEqual(data, Data())
        XCTAssertEqual(requested, 65_536)
    }

    func testNegativeLimitDoesNotRead() {
        var called = false
        XCTAssertThrowsError(try HvfWindowsInstallRegularFile.collect(maximumBytes: -1) { _ in
            called = true
            return Data()
        })
        XCTAssertFalse(called)
    }

    private func fixture(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root)
    }
}
