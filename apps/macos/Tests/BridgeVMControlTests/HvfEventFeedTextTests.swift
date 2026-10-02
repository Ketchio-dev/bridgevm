import XCTest
@testable import BridgeVMControl

final class HvfEventFeedTextTests: XCTestCase {
    func testOnlyTheNewestEventsAreDrawnAndLongLinesAreCut() {
        let events = (0..<500).map { BvAgentEvent.unknown("line \($0)") } + [.unknown(String(repeating: "x", count: 1000))]
        let lines = HvfEventFeedText.render(events).split(separator: "\n", omittingEmptySubsequences: false)
        XCTAssertEqual(lines.count, HvfEventFeedText.shownEvents)
        XCTAssertEqual(lines.first, "line 301")
        XCTAssertEqual(lines.last?.count, HvfEventFeedText.lineLimit + 1)
        XCTAssertTrue(lines.last?.hasSuffix("…") == true)
    }
    func testCommandBodiesKeepTheirLinesAndShortTextIsUnchanged() {
        XCTAssertEqual(HvfEventFeedText.render([.serviceStart(tMs: 2), .commandOutput(label: "whoami", body: "a\nb")]),
                       "SERVICE start t=2\nCMD whoami\na\nb")
        XCTAssertEqual(HvfEventFeedText.render([]), "")
    }
}
