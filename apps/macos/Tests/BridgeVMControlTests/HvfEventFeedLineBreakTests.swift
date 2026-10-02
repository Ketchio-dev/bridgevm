import XCTest
@testable import BridgeVMControl

final class HvfEventFeedLineBreakTests: XCTestCase {
    func testEveryHardLineSeparatorUsesTheNewestDisplayedLineBudget() {
        for separator in ["\n", "\r", "\r\n", "\u{000B}", "\u{000C}", "\u{0085}", "\u{2028}", "\u{2029}"] {
            let body = (0..<300).map { "row \($0)" }.joined(separator: separator)
            let event = BvAgentEvent.commandOutput(label: "mixed", body: body)
            let expected = (100..<300).map { "row \($0)" }.joined(separator: "\n")
            XCTAssertEqual(HvfEventFeedText.render([event]), expected)
            XCTAssertEqual(event.displayText, "CMD mixed\n\(body)")
        }
    }

    func testMixedSeparatorsAndEmptyLinesKeepChronologicalOrder() {
        let text = "old\r\n\rA\u{0085}B\u{2028}\u{2029}C\u{000C}D\u{000B}last\r\n"
        XCTAssertEqual(HvfEventFeedLines.render([text], count: 8, length: 240), "\nA\nB\n\nC\nD\nlast\n")
        XCTAssertEqual(HvfEventFeedLines.render([text], count: 3, length: 240), "D\nlast\n")
    }

    func testCRLFPairsAreOneBoundaryBesideStandaloneCarriageReturns() {
        XCTAssertEqual(HvfEventFeedLines.render(["a\r\r\nb\n\rc"], count: 20, length: 240), "a\n\nb\n\nc")
        XCTAssertEqual(HvfEventFeedLines.render(["\r\n"], count: 20, length: 240), "\n")
        XCTAssertEqual(HvfEventFeedLines.render(["\r\n"], count: 1, length: 240), "")
    }

    func testLineBudgetSpansEventsWithDifferentSeparators() {
        let earlier = (0..<150).map { "old \($0)" }.joined(separator: "\u{2028}")
        let later = (0..<150).map { "new \($0)" }.joined(separator: "\r")
        let expected = (100..<150).map { "old \($0)" } + (0..<150).map { "new \($0)" }
        XCTAssertEqual(HvfEventFeedText.render([.unknown(earlier), .unknown(later)]), expected.joined(separator: "\n"))
    }
}
