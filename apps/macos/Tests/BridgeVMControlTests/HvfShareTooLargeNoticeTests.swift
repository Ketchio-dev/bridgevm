import Foundation
import XCTest
@testable import BridgeVMControl

/// A shared-folder file over the size limit was only a log line; the runtime card now says so.
final class HvfShareTooLargeNoticeTests: XCTestCase {
    @MainActor
    func testATooLargeSkipShowsTheFileAndTheLimit() {
        let notice = HvfShareRootNotice()
        XCTAssertNil(notice.displayed)
        notice.consume(.unknown("BVAGENT SHARE skip 휴가 영상.mov too-large 200000000\r"))
        XCTAssertEqual(notice.tooLarge, HvfShareTooLarge(count: 1, last: "휴가 영상.mov"))
        XCTAssertEqual(notice.displayed?.text, "16 GB보다 큰 파일은 공유 폴더로 옮기지 않습니다: 휴가 영상.mov")
        notice.consume(.unknown("BVAGENT SHARE skip big/disk.iso too-large 5000000000"))
        XCTAssertEqual(notice.displayed?.text, "16 GB보다 큰 파일은 공유 폴더로 옮기지 않습니다: big/disk.iso 외 1개")
    }

    @MainActor
    func testOtherSkipsAndCommandOutputAreNotTooLarge() {
        let notice = HvfShareRootNotice()
        // A Mac file name can hold a newline, so a whole line can come from one: "skip too-large 1"
        // has its marker on the prefix's own space.
        for line in ["BVAGENT SHARE skip foo alias [\"foo\", \"foo.\"]", "BVAGENT SHARE skip  too-large 9",
                     "BVAGENT SHARE skip a.bin too-large ", "BVAGENT SHARE skip a.bin too-large 12x",
                     "BVAGENT SHARE skip too-large 123", "BVAGENT SHARE skip a.bin too-large \u{0663}"] {
            notice.consume(.unknown(line))
        }
        for event in BvAgentEvent.parse(lines: ["BVAGENT CMD type log.txt exit=0",
                                                "BVAGENT SHARE skip a.bin too-large 99", "BVAGENT END type log.txt"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.tooLarge)
    }

    @MainActor
    func testAStopOrABlockedRootOutranksItAndResetClearsIt() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown("BVAGENT SHARE skip a.bin too-large 99999999"))
        notice.consume(.unknown("BVAGENT SHARE root-state blocked"))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.")
        notice.consume(.unknown("BVAGENT SHARE root-state ready"))
        XCTAssertNotNil(notice.tooLarge)
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.tooLarge)
        XCTAssertNil(notice.displayed)
    }

    func testTheRunnerGetsTheSameLimitTheNoticeNames() {
        XCTAssertEqual(HvfEngineConfig.shareMaxKiB, 16 * 1024 * 1024)
    }
}
