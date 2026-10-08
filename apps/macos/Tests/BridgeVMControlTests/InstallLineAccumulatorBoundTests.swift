import Foundation
import XCTest
@testable import BridgeVMControl

final class InstallLineAccumulatorBoundTests: XCTestCase {
    private let limit = 2 * 1_048_576

    func testNewlineFreeChunksKeepOnlyBoundedPendingBytes() throws {
        let accumulator = LineAccumulator()
        let chunk = Data(repeating: 120, count: 1_048_576)
        for index in 0..<16 {
            XCTAssertTrue(accumulator.append(chunk).isEmpty)
            let bytes = try XCTUnwrap(Mirror(reflecting: accumulator).children.first {
                $0.label == "buffer"
            }?.value as? Data)
            XCTAssertLessThanOrEqual(bytes.count, limit)
            if index >= 2 { XCTAssertEqual(bytes.count, 0) }
        }
    }

    func testOversizedLineAndItsSuffixAreNotEmittedAsRecords() {
        let accumulator = LineAccumulator()
        XCTAssertTrue(accumulator.append(Data(repeating: 120, count: limit + 1)).isEmpty)
        let lines = accumulator.append(Data("BVAGENT fake\nok\n".utf8))
        XCTAssertEqual(lines.count, 1)
        XCTAssertEqual(lines.last, "ok")
        XCTAssertFalse(lines.contains { $0.contains("BVAGENT fake") })
    }
}
