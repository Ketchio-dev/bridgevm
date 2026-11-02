import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// The runtime's exact share-disabled lines become one informational notice per boot generation.
final class HvfShareStopNoticeTests: XCTestCase {
    private let tooOld = "BVAGENT SHARE agent-too-old (needs v3-share2); share disabled"
    private let failed = "BVAGENT SHARE disabled failed-destination-limit"

    @MainActor
    func testExactStopLinesSetTheirReasonAndVariantsDoNot() {
        let notice = HvfShareRootNotice()
        for variant in [tooOld + " ", "BVAGENT SHARE agent-too-old", failed + " t=1", "BVAGENT SHARE disabled",
                        "BVAGENT SHARE disabled failed-destination-limit\r\r"] {
            notice.consume(.unknown(variant))
            XCTAssertNil(notice.stopped, variant)
        }
        notice.consume(.unknown(tooOld + "\r"))
        XCTAssertEqual(notice.stopped, .agentTooOld)
        notice.reset()
        notice.consume(.unknown(failed))
        XCTAssertEqual(notice.stopped, .failedDestinations)
    }

    @MainActor
    func testTheFirstStopReasonStaysUntilResetAndGuestResetClearsIt() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown(failed))
        notice.consume(.unknown(tooOld))
        XCTAssertEqual(notice.stopped, .failedDestinations)
        notice.consume(.unknown("BVAGENT SHARE root-state ready"))
        XCTAssertEqual(notice.stopped, .failedDestinations, "root recovery does not re-enable a stopped share")
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.stopped)
        XCTAssertFalse(notice.isBlocked)
    }

    @MainActor
    func testStopLinesInsideCommandOutputAreIgnored() {
        let notice = HvfShareRootNotice()
        for event in BvAgentEvent.parse(lines: ["BVAGENT CMD type log.txt exit=0", tooOld, failed,
                                                "BVAGENT END type log.txt"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.stopped)
    }

    @MainActor
    func testStopPublishesOnlyOnChange() {
        let notice = HvfShareRootNotice()
        var published: [HvfShareStop?] = []
        let sink = notice.$stopped.dropFirst().sink { published.append($0) }
        defer { sink.cancel() }
        notice.reset()
        notice.consume(.unknown(tooOld))
        notice.consume(.unknown(tooOld))
        notice.consume(.unknown(failed))
        notice.reset()
        notice.reset()
        XCTAssertEqual(published, [.agentTooOld, nil])
    }
}
