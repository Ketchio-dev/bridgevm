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
        XCTAssertEqual(notice.displayed?.text, "Sharing skipped “휴가 영상.mov” because the per-file limit is 16 GiB.")
        notice.consume(.unknown("BVAGENT SHARE skip big/disk.iso too-large 5000000000"))
        XCTAssertEqual(notice.displayed?.text, "Sharing skipped “big/disk.iso” because the per-file limit is 16 GiB. Folder sharing has skipped oversized files 2 times this session.")
        notice.consume(.unknown("BVAGENT SHARE skip big/disk.iso too-large 18000000000"))
        XCTAssertEqual(notice.displayed?.text, "Sharing skipped “big/disk.iso” because the per-file limit is 16 GiB. Folder sharing has skipped oversized files 3 times this session.")
    }

    /// Before Guest Tools move files in pieces the limit is 64 MiB, not 16 GiB, and the file syncs
    /// after the automatic update; a Windows file cannot be dragged from the Mac.
    @MainActor
    func testTheNoticeNamesTheLimitThatAppliedAndClearsWhenTheFileSyncs() {
        let notice = HvfShareRootNotice()
        // The line share.rs too_large_line writes (share_skip_line_tests.rs).
        notice.consume(.unknown("BVAGENT SHARE skip 휴가 영상.mov too-large 200000000 limit=67108864 mac"))
        XCTAssertEqual(notice.displayed?.text, "Sharing skipped “휴가 영상.mov” because the per-file limit is 64 MiB.")
        XCTAssertEqual(notice.displayed?.help,
                       "Larger files sync after BridgeVM updates Guest Tools in Windows, which it does automatically.")
        for event in BvAgentEvent.parse(lines: ["BVAGENT SHARE host->guest 휴가 영상.mov bytes=200000000 t=9000"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.tooLarge, "the update raised the limit and the file arrived")
        notice.consume(.unknown("BVAGENT SHARE skip disk.vhdx too-large 18000000000 limit=17179869184 windows"))
        XCTAssertEqual(notice.displayed?.text, "Sharing skipped “disk.vhdx” because the per-file limit is 16 GiB.")
        XCTAssertEqual(notice.displayed?.help, "To share a larger file from Windows, split or compress it first.")
        notice.consume(.unknown("BVAGENT SHARE skip a limit=5 mac too-large 18000000000 limit=17179869184 mac"))
        XCTAssertEqual(notice.tooLarge, HvfShareTooLarge(count: 2, last: "a limit=5 mac", limit: 17_179_869_184))
        XCTAssertEqual(notice.displayed?.help, "To copy a large file from your Mac to Windows, drag it onto the VM window.")
    }

    @MainActor
    func testOtherSkipsAndCommandOutputAreNotTooLarge() {
        let notice = HvfShareRootNotice()
        // A Mac file name can hold a newline, so a whole line can come from one: "skip too-large 1"
        // has its marker on the prefix's own space.
        for line in ["BVAGENT SHARE skip foo alias [\"foo\", \"foo.\"]", "BVAGENT SHARE skip  too-large 9",
                     "BVAGENT SHARE skip a.bin too-large ", "BVAGENT SHARE skip a.bin too-large 12x",
                     "BVAGENT SHARE skip too-large 123", "BVAGENT SHARE skip a.bin too-large \u{0663}",
                     "BVAGENT SHARE skip a.bin too-large 9 limit= mac", "BVAGENT SHARE skip a.bin too-large 9 limit=1x windows"] {
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
        XCTAssertEqual(notice.displayed?.text, "Some changes are waiting because BridgeVM can’t find the shared folder on your Mac.")
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
