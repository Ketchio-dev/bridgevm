import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// A guest folder over the listing limit pauses the share; the runtime card says so, with the
/// limit the runtime names, instead of leaving only an ls-error line in run.log.
final class HvfSharePausedNoticeTests: XCTestCase {
    private let entries = "BVAGENT SHARE paused too-many-entries limit=4096"

    @MainActor
    func testThePausedLineShowsTheLimitItNames() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown(entries + "\r"))
        XCTAssertEqual(notice.paused, HvfSharePaused(reason: .tooManyEntries, limit: 4096))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because the Windows folder has more than \(4096.formatted()) items.")
        XCTAssertEqual(notice.displayed?.help, "Move some items out of the Windows folder, or shut down the VM and choose a smaller shared folder.")
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        notice.consume(.unknown("BVAGENT SHARE paused too-many-entries limit=20000"))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because the Windows folder has more than \(20000.formatted()) items.")
        notice.consume(.unknown("BVAGENT SHARE paused too-many-bytes limit=1048576 t=9"))
        XCTAssertEqual(notice.paused, HvfSharePaused(reason: .tooManyBytes, limit: 1_048_576))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because the Windows file listing exceeds \(1048576.formatted()) bytes.")
    }

    @MainActor
    func testResumedClearsItAStopOutranksItAndAGuestResetClearsIt() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown(entries))
        notice.consume(.unknown("BVAGENT SHARE root-state blocked"))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because the Windows folder has more than \(4096.formatted()) items.")
        notice.consume(.unknown("BVAGENT SHARE resumed\r"))
        XCTAssertNil(notice.paused)
        XCTAssertEqual(notice.displayed?.text, "Some changes are waiting because BridgeVM can’t verify the shared folder.")
        notice.consume(.unknown(entries))
        notice.consume(.unknown("BVAGENT SHARE disabled failed-destination-limit"))
        XCTAssertEqual(notice.displayed?.text, "Folder sync stopped for this session after repeated write failures on your Mac.")
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.paused)
        XCTAssertNil(notice.displayed)
    }

    @MainActor
    func testVariantsAndCommandOutputAreNotPaused() {
        let notice = HvfShareRootNotice()
        for line in ["BVAGENT SHARE paused", "BVAGENT SHARE paused too-many-entries", "BVAGENT SHARE paused  limit=5",
                     "BVAGENT SHARE paused too-many-entries limit=", "BVAGENT SHARE paused too-many-entries limit=40x",
                     "BVAGENT SHARE paused too-many-entries limit=0", "BVAGENT SHARE paused other limit=5",
                     "BVAGENT SHARE paused too-many-entries limit=\u{0664}"] {
            notice.consume(.unknown(line))
        }
        for event in BvAgentEvent.parse(lines: ["BVAGENT CMD type log.txt exit=0", entries, "BVAGENT END type log.txt"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.paused)
    }

    @MainActor
    func testItPublishesOnlyOnChange() {
        let notice = HvfShareRootNotice()
        var published: [HvfSharePaused?] = []
        let sink = notice.$paused.dropFirst().sink { published.append($0) }
        defer { sink.cancel() }
        notice.consume(.unknown(entries))
        notice.consume(.unknown(entries))
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        XCTAssertEqual(published, [HvfSharePaused(reason: .tooManyEntries, limit: 4096), nil])
    }
}
