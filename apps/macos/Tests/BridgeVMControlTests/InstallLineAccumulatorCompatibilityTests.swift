import Foundation
import XCTest
@testable import BridgeVMControl

final class InstallLineAccumulatorCompatibilityTests: XCTestCase {
    func testExactLimitAndOverflowAtSeparateNewline() {
        let limit = LineAccumulator.maximumLineBytes
        let accumulator = LineAccumulator()
        XCTAssertEqual(accumulator.append(Data(repeating: 120, count: limit)), [])
        let full = accumulator.append(Data([10]))
        XCTAssertEqual(full.count, 1)
        XCTAssertEqual(full.first?.utf8.count, limit)
        XCTAssertEqual(accumulator.append(Data(repeating: 120, count: limit)), [])
        XCTAssertEqual(accumulator.append(Data("y\nsuffix\n".utf8)), ["suffix"])
    }

    func testOversizedRecordsInSingleBurstAndAcrossDiscardedChunks() {
        let accumulator = LineAccumulator()
        let large = Data(repeating: 120, count: LineAccumulator.maximumLineBytes + 1)
        var burst = Data("before\n".utf8)
        burst.append(large)
        burst.append(Data("\nafter\n\n".utf8))
        XCTAssertEqual(accumulator.append(burst), ["before", "after"])
        XCTAssertEqual(accumulator.append(large), [])
        XCTAssertEqual(accumulator.append(Data("discarded".utf8)), [])
        XCTAssertEqual(accumulator.append(Data()), [])
        XCTAssertEqual(accumulator.append(Data("tail\nnext\n".utf8)), ["next"])
    }

    func testSplitUTF8InvalidBytesBlankLinesAndCRRemainCompatible() {
        let accumulator = LineAccumulator()
        let bytes = Array("가".utf8)
        XCTAssertEqual(accumulator.append(Data(bytes.prefix(1))), [])
        XCTAssertEqual(accumulator.append(Data(bytes.dropFirst()) + Data("\r\n\n\r\n".utf8)), ["가\r", "\r"])
        XCTAssertEqual(accumulator.append(Data([0xff, 10]) + Data("valid\n".utf8)), ["valid"])
        XCTAssertEqual(accumulator.append(Data("incomplete".utf8)), [])
        XCTAssertEqual(accumulator.append(Data()), [])
        XCTAssertEqual(accumulator.append(Data("-line\n".utf8)), ["incomplete-line"])
    }

    func testLargeDeliveryStillPreservesEveryShortLine() {
        let accumulator = LineAccumulator()
        let line = String(repeating: "x", count: 1024)
        let data = Data(String(repeating: line + "\n", count: 2200).utf8)
        XCTAssertGreaterThan(data.count, LineAccumulator.maximumLineBytes)
        let lines = accumulator.append(data)
        XCTAssertEqual(lines.count, 2200)
        XCTAssertTrue(lines.allSatisfy { $0 == line })
    }

    func testCarriageReturnCountsTowardLimitAndReplacementCharacterIsValid() {
        let accumulator = LineAccumulator()
        let data = Data(repeating: 120, count: LineAccumulator.maximumLineBytes)
        XCTAssertEqual(accumulator.append(data), [])
        XCTAssertEqual(accumulator.append(Data("\r\n\u{fffd}\na\rb\n".utf8)), ["\u{fffd}", "a\rb"])
    }

    func testConcurrentCompleteRecordsAreNotLost() {
        let accumulator = LineAccumulator()
        let delivery = InstallLineDeliveryFixture()
        DispatchQueue.concurrentPerform(iterations: 32) { index in
            delivery.append(accumulator.append(Data("line-\(index)\n".utf8)))
        }
        let lines = delivery.values()
        XCTAssertEqual(lines.count, 32)
        XCTAssertEqual(Set(lines), Set((0..<32).map { "line-\($0)" }))
        XCTAssertEqual(accumulator.append(Data()), [])
    }
}

private final class InstallLineDeliveryFixture: @unchecked Sendable {
    private let lock = NSLock()
    private var lines: [String] = []
    func append(_ value: [String]) {
        lock.lock()
        defer { lock.unlock() }
        lines.append(contentsOf: value)
    }
    func values() -> [String] {
        lock.lock()
        defer { lock.unlock() }
        return lines
    }
}
