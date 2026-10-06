import XCTest
@testable import BridgeVMControl

final class HvfEventFeedUnicodeTests: XCTestCase {
    func testOneHugeGraphemeIsBoundedWithoutChangingTheRetainedEvent() {
        let body = "a" + String(repeating: "\u{0301}", count: 1_048_576)
        let event = BvAgentEvent.commandOutput(label: "unicode", body: body)
        let text = HvfEventFeedText.render([event])
        let last = text.split(separator: "\n").last!
        let limit = HvfEventFeedLinePrefix.scalarLimit
        XCTAssertEqual(String(last), "a" + String(repeating: "\u{0301}", count: limit - 1) + "…")
        XCTAssertEqual(last.unicodeScalars.count, limit + 1)
        XCTAssertEqual(last.utf8.count, 1 + (limit - 1) * 2 + 3)
        XCTAssertEqual([event], [.commandOutput(label: "unicode", body: body)])
        XCTAssertEqual(event.displayText, "CMD unicode\n\(body)")
    }

    func testExactlyBoundedGraphemeKeepsItsBytesAndOverflowGetsAnEllipsis() {
        let body = "a" + String(repeating: "\u{0301}", count: HvfEventFeedLinePrefix.scalarLimit - 1)
        XCTAssertEqual(render(body), body)
        XCTAssertEqual(render(body + "\u{0301}"), body + "…")
    }

    func testFourByteScalarsRespectTheDeclaredMaximumUTF8Bytes() {
        let body = "😀" + String(repeating: "\u{1D185}", count: 10_000)
        let text = render(body)
        XCTAssertEqual(text.unicodeScalars.count, HvfEventFeedLinePrefix.scalarLimit + 1)
        XCTAssertEqual(text.utf8.count, HvfEventFeedLinePrefix.scalarLimit * 4 + 3)
        XCTAssertTrue(text.hasSuffix("…"))
    }

    func testScalarAndGraphemeLimitsBothApply() {
        let body = String(repeating: "x", count: 500) + String(repeating: "\u{0301}", count: 10_000)
        XCTAssertEqual(render(body), String(repeating: "x", count: HvfEventFeedText.lineLimit) + "…")
        let glyph = "👩🏽‍💻"
        XCTAssertEqual(render(String(repeating: glyph, count: HvfEventFeedText.lineLimit)),
            String(repeating: glyph, count: HvfEventFeedText.lineLimit))
    }

    func testScalarNewlineSearchKeepsNewestEmptyAndUnicodeLinesInOrder() {
        let body = "a" + String(repeating: "\u{0301}", count: 10_000)
        XCTAssertEqual(HvfEventFeedLines.render(["old\n\n\(body)\nlast\n"], count: 4, length: 240),
            "\n\(render(body))\nlast\n")
        XCTAssertEqual(HvfEventFeedLines.render(["older\r\nnew\r\n"], count: 2, length: 240), "new\n")
    }

    func testPrefixDoesNotReadTheWholeScalarCollectionBeforeCountingCharacters() {
        let reads = ScalarReads()
        let text = HvfEventFeedLinePrefix.render(CountingScalars(reads: reads), length: 240)
        XCTAssertEqual(reads.value, HvfEventFeedLinePrefix.scalarLimit + 1)
        XCTAssertEqual(text.unicodeScalars.count, HvfEventFeedLinePrefix.scalarLimit + 1)
        XCTAssertTrue(text.hasSuffix("…"))
    }

    private func render(_ body: String) -> String {
        HvfEventFeedLines.render([body], count: HvfEventFeedText.shownLines, length: HvfEventFeedText.lineLimit)
    }
}

private final class ScalarReads {
    var value = 0
}

private struct CountingScalars: Collection {
    let reads: ScalarReads
    var startIndex: Int { 0 }
    var endIndex: Int { 1_048_577 }
    func index(after index: Int) -> Int { index + 1 }
    subscript(index: Int) -> UnicodeScalar {
        reads.value += 1
        return index == 0 ? "a" : "\u{0301}"
    }
}
