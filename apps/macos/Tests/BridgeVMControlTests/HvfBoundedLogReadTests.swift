import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfBoundedLogReadTests: XCTestCase {
    private let limit = 1_048_576

    func testBacklogIsConsumedAcrossCallsInsteadOfOneUnboundedRead() throws {
        let first = String(repeating: "x", count: limit - 1)
        try withLog(Data((first + "\nsecond\n").utf8)) { url in
            let reader = TailOffsetReader()
            XCTAssertEqual(reader.readNewLines(from: url), [first])
            XCTAssertEqual(reader.readNewLines(from: url), ["second"])
            XCTAssertEqual(reader.readNewLines(from: url), [])
        }
    }

    func testUTF8AndIncompleteLineSurviveTheReadBoundary() throws {
        let line = String(repeating: "x", count: limit - 1) + "\u{d55c}"
        try withLog(Data((line + "\r\nnext\n").utf8)) { url in
            let reader = TailOffsetReader()
            XCTAssertEqual(reader.readNewLines(from: url), [])
            XCTAssertEqual(reader.readNewLines(from: url), [line, "next"])
        }
    }

    func testStartingCursorSkipsHistoryWithoutSkippingNewLines() throws {
        let old = Data("historical\n".utf8)
        try withLog(old + Data("current\n".utf8)) { url in
            let reader = TailOffsetReader(startingAt: UInt64(old.count))
            XCTAssertEqual(reader.readNewLines(from: url), ["current"])
        }
    }

    private func withLog(_ data: Data, body: (URL) throws -> Void) throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        try body(url)
    }
}
