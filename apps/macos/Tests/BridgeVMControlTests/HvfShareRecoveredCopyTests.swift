import XCTest
@testable import BridgeVMControl

final class HvfShareRecoveredCopyTests: XCTestCase {
    @MainActor
    func testRecoveredSiblingKeepsItsExactNameAlongsideExistingWarningUntilReset() {
        let notice = HvfShareRootNotice()
        var stream = BvAgentEvent.Stream()
        let name = "회의 windows-copy=Q3 예산.최종.xlsx"
        let copy = "회의 windows-copy=Q3 예산.최종 (Windows copy 2).xlsx"
        let marker = "BVAGENT SHARE conflict kept-mac=\(name) windows-copy=\(copy)"

        for event in stream.parse(["BVAGENT SHARE root-state blocked", marker + "\r"]) {
            notice.consume(event)
        }
        XCTAssertEqual(notice.lastRecoveredCopy, copy)
        XCTAssertTrue(notice.isBlocked)
        XCTAssertNotNil(notice.displayed)

        notice.reset()
        XCTAssertNil(notice.lastRecoveredCopy)
        for event in stream.parse(["BVAGENT CMD sample exit=0", marker, "BVAGENT END sample",
                                   "BVAGENT SHARE conflict windows-copy-not-kept=\(name) no space",
                                   "BVAGENT SHARE conflict kept-mac=doc.txt windows-copy=../../other.txt"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.lastRecoveredCopy)
    }
}
