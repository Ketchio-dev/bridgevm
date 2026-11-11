import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// A guest folder over the listing limit paused the share with only an ls-error line in run.log
/// (share-bigloss-20261111); the runtime card now says so, with the limit the runtime names.
final class HvfSharePausedNoticeTests: XCTestCase {
    private let entries = "BVAGENT SHARE paused too-many-entries limit=4096"

    @MainActor
    func testThePausedLineShowsTheLimitItNames() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown(entries + "\r"))
        XCTAssertEqual(notice.paused, HvfSharePaused(reason: .tooManyEntries, limit: 4096))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더에 항목이 너무 많아(4,096개 초과) 동기화를 멈추었습니다.")
        XCTAssertEqual(notice.displayed?.help, "VM을 종료한 뒤 Mac과 Windows 모두 항목이 더 적은 폴더를 고르면 다시 동기화됩니다.")
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        notice.consume(.unknown("BVAGENT SHARE paused too-many-entries limit=20000"))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더에 항목이 너무 많아(20,000개 초과) 동기화를 멈추었습니다.")
        notice.consume(.unknown("BVAGENT SHARE paused too-many-bytes limit=1048576 t=9"))
        XCTAssertEqual(notice.paused, HvfSharePaused(reason: .tooManyBytes, limit: 1_048_576))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더에 항목이 너무 많거나 이름이 너무 길어 동기화를 멈추었습니다.")
    }

    @MainActor
    func testResumedClearsItAStopOutranksItAndAGuestResetClearsIt() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown(entries))
        notice.consume(.unknown("BVAGENT SHARE root-state blocked"))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더에 항목이 너무 많아(4,096개 초과) 동기화를 멈추었습니다.")
        notice.consume(.unknown("BVAGENT SHARE resumed\r"))
        XCTAssertNil(notice.paused)
        XCTAssertEqual(notice.displayed?.text, "공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.")
        notice.consume(.unknown(entries))
        notice.consume(.unknown("BVAGENT SHARE disabled failed-destination-limit"))
        XCTAssertEqual(notice.displayed?.text, "Mac 폴더에 쓰기가 반복해서 실패해 이번 실행에서 공유 폴더 동기화를 중지했습니다.")
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
