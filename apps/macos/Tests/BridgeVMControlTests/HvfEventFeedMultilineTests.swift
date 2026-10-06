import XCTest
@testable import BridgeVMControl

final class HvfEventFeedMultilineTests: XCTestCase {
    func testOneHugeCommandDrawsOnlyTheNewestLinesWithoutChangingTheEvent() {
        let body = (0..<100_000).map { "synthetic-line-\($0)" }.joined(separator: "\n")
        let event = BvAgentEvent.commandOutput(label: "synthetic", body: body)
        let events = [event]
        let lines = renderedLines(events)
        XCTAssertEqual(lines.count, HvfEventFeedText.shownLines)
        XCTAssertEqual(lines.first, "synthetic-line-99800")
        XCTAssertEqual(lines.last, "synthetic-line-99999")
        XCTAssertEqual(events, [event])
        XCTAssertEqual(event.displayText, "CMD synthetic\n\(body)")
    }

    func testLineBudgetSpansEventsAndKeepsTheNewestEvent() {
        let first = BvAgentEvent.commandOutput(label: "earlier", body: (0..<300).map { "row \($0)" }.joined(separator: "\n"))
        let events: [BvAgentEvent] = [first, .commandOutput(label: "later", body: "latest\nlast")]
        let lines = renderedLines(events)
        XCTAssertEqual(lines.count, HvfEventFeedText.shownLines)
        XCTAssertEqual(lines.first, "row 103")
        XCTAssertEqual(Array(lines.suffix(3)), ["CMD later", "latest", "last"])
    }

    func testEveryVisibleLineIsTruncatedWithinTheTotalTextBudget() {
        let body = Array(repeating: String(repeating: "x", count: 10_000), count: 300).joined(separator: "\n")
        let text = HvfEventFeedText.render([.commandOutput(label: "synthetic", body: body)])
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        XCTAssertEqual(lines.count, HvfEventFeedText.shownLines)
        XCTAssertTrue(lines.allSatisfy { $0.count == HvfEventFeedText.lineLimit + 1 && $0.hasSuffix("…") })
        XCTAssertEqual(text.count, HvfEventFeedText.shownLines * (HvfEventFeedText.lineLimit + 2) - 1)
    }

    func testEmptyAndUnicodeLinesKeepTheirChronologicalOrder() {
        let glyph = "👩🏽‍💻"
        let body = "\n" + String(repeating: glyph, count: HvfEventFeedText.lineLimit + 1) + "\n"
        XCTAssertEqual(HvfEventFeedText.render([.commandOutput(label: "unicode", body: body)]),
            "CMD unicode\n\n" + String(repeating: glyph, count: HvfEventFeedText.lineLimit) + "…\n")
        XCTAssertEqual(HvfEventFeedText.render([.unknown("")]), "")
    }

    func testCollectorStopsBeforeReadingOlderEventsOnceItIsFull() {
        var visited = 0
        let texts = ["old", "older", "newest\nlast"].reversed().lazy.map { text in visited += 1; return text }
        XCTAssertEqual(HvfEventFeedLines.render(texts, count: 2, length: 240), "newest\nlast")
        XCTAssertEqual(visited, 1)
    }

    private func renderedLines(_ events: [BvAgentEvent]) -> [String] {
        HvfEventFeedText.render(events).split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
    }
}
