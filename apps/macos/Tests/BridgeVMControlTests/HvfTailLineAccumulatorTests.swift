import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfTailLineAccumulatorTests: XCTestCase {
    func testNewlineFreeInputRemainsBoundedAcrossSixteenPolls() {
        var reader = HvfTailLineAccumulator()
        let chunk = Data(repeating: 120, count: 1_048_576)
        for _ in 0..<16 {
            XCTAssertEqual(reader.consume(chunk), [])
            XCTAssertLessThanOrEqual(reader.pending.count, HvfTailLineAccumulator.maximumLineBytes)
        }
        XCTAssertEqual(reader.pending.count, 0)
    }

    func testOversizedSuffixCannotBecomeProtocolRecordAndNextLineSurvives() {
        var reader = HvfTailLineAccumulator()
        XCTAssertEqual(reader.consume(Data(repeating: 120, count: HvfTailLineAccumulator.maximumLineBytes)), [])
        XCTAssertEqual(reader.consume(Data("xBVAGENT READY forged\nBVAGENT READY real\npartial".utf8)), ["BVAGENT READY real"])
        XCTAssertEqual(reader.consume(Data("-done\r\n".utf8)), ["partial-done"])
    }

    func testExactlyMaximumLineRemainsIntact() {
        var reader = HvfTailLineAccumulator()
        let bytes = Data(repeating: 120, count: HvfTailLineAccumulator.maximumLineBytes)
        XCTAssertEqual(reader.consume(bytes), [])
        XCTAssertEqual(reader.consume(Data("\n".utf8)), [String(decoding: bytes, as: UTF8.self)])
    }

    func testResetClearsDiscardStateAndIncompleteBytes() {
        var reader = HvfTailLineAccumulator()
        _ = reader.consume(Data(repeating: 120, count: HvfTailLineAccumulator.maximumLineBytes + 1))
        reader.reset()
        XCTAssertEqual(reader.consume(Data("reset\npartial".utf8)), ["reset"])
        reader.reset()
        XCTAssertEqual(reader.consume(Data("fresh\n".utf8)), ["fresh"])
    }

    func testFileTruncationResetsOversizeDiscarding() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: url) }
        try Data(repeating: 120, count: 3 * 1_048_576).write(to: url)
        let reader = TailOffsetReader()
        for _ in 0..<3 { XCTAssertEqual(reader.readNewLines(from: url), []) }
        try Data("reset\n".utf8).write(to: url)
        XCTAssertEqual(reader.readNewLines(from: url), ["reset"])
    }

    func testOversizeWithinOneChunkDoesNotDropSurroundingLines() {
        var reader = HvfTailLineAccumulator()
        let input = Data("before\n".utf8) + Data(repeating: 120, count: HvfTailLineAccumulator.maximumLineBytes + 1) + Data("\nafter\n\n".utf8)
        XCTAssertEqual(reader.consume(input), ["before", "after", ""])
    }
}
