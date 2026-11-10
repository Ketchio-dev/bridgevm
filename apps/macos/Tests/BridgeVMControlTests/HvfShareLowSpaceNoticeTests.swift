import Foundation
import XCTest
@testable import BridgeVMControl

/// A Windows file the Mac had no room for was only a log line; the runtime card now says so until
/// the file arrives.
final class HvfShareLowSpaceNoticeTests: XCTestCase {
    /// The lines as the session gets them from run.log.
    @MainActor
    private func feed(_ notice: HvfShareRootNotice, _ lines: [String]) {
        for event in BvAgentEvent.parse(lines: lines) { notice.consume(event) }
    }

    @MainActor
    func testALowSpaceSkipNamesTheFileAndSaysItComesOnceThereIsRoom() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown("BVAGENT SHARE skip 휴가 영상.mov low-space 5000000000\r"))
        XCTAssertEqual(notice.lowSpace?.count, 1)
        XCTAssertEqual(notice.lowSpace?.last, "휴가 영상.mov")
        XCTAssertEqual(notice.displayed?.text, "Mac 디스크 공간이 부족해 '휴가 영상.mov'을(를) 공유 폴더로 가져오지 않았습니다.")
        XCTAssertEqual(notice.displayed?.help, "공간을 확보하면 다시 가져옵니다.")
        notice.consume(.unknown("BVAGENT SHARE skip big/disk.iso low-space 9000000000"))
        XCTAssertEqual(notice.displayed?.text, "Mac 디스크 공간이 부족해 'big/disk.iso' 외 1개를 공유 폴더로 가져오지 않았습니다.")
        // It outranks a too-large skip, which needs nothing from the user.
        notice.consume(.unknown("BVAGENT SHARE skip a.bin too-large 99999999999"))
        XCTAssertEqual(notice.lowSpace?.count, 2)
        XCTAssertTrue(notice.displayed?.text.hasPrefix("Mac 디스크 공간이 부족해") == true)
    }

    @MainActor
    func testOnlyTheExactLineWithASCIIDigitsCounts() {
        let notice = HvfShareRootNotice()
        // A name can hold a newline, so a whole line can come from one: "skip low-space 1" has its
        // marker on the prefix's own space.
        for line in ["BVAGENT SHARE skip low-space 123", "BVAGENT SHARE skip  low-space 9",
                     "BVAGENT SHARE skip a.bin low-space ", "BVAGENT SHARE skip a.bin low-space 12x",
                     "BVAGENT SHARE skip a.bin low-space \u{0663}", "BVAGENT SHARE skip a.bin too-large 5",
                     "BVAGENT SHARE skipa.bin low-space 5", "xBVAGENT SHARE skip a.bin low-space 5"] {
            notice.consume(.unknown(line))
        }
        XCTAssertNil(notice.lowSpace)
        // A name that holds the other marker is still this file's.
        XCTAssertEqual(HvfShareLowSpace.name(in: "BVAGENT SHARE skip a too-large 5 low-space 99"), "a too-large 5")
        XCTAssertNil(HvfShareTooLarge.name(in: "BVAGENT SHARE skip a too-large 5 low-space 99"))
    }

    @MainActor
    func testAStopOrABlockedRootOutranksItAndResetClearsIt() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown("BVAGENT SHARE skip a.bin low-space 99999999999"))
        notice.consume(.unknown("BVAGENT SHARE root-state blocked"))
        XCTAssertEqual(notice.displayed?.text, "공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.")
        notice.consume(.unknown("BVAGENT SHARE root-state ready"))
        XCTAssertNotNil(notice.lowSpace)
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.lowSpace)
        XCTAssertNil(notice.displayed)
    }

    /// The runtime reports each version of a file once; it is still one file.
    @MainActor
    func testTwoVersionsOfOneFileCountOnce() {
        let notice = HvfShareRootNotice()
        feed(notice, ["BVAGENT SHARE skip a.txt low-space 5", "BVAGENT SHARE skip b.txt low-space 6",
                      "BVAGENT SHARE skip a.txt low-space 7"])
        XCTAssertEqual(notice.lowSpace?.count, 2)
        XCTAssertEqual(notice.lowSpace?.last, "a.txt")
        XCTAssertEqual(notice.displayed?.text, "Mac 디스크 공간이 부족해 'a.txt' 외 1개를 공유 폴더로 가져오지 않았습니다.")
    }

    @MainActor
    func testAnArrivalClearsItsFileAndTheLastArrivalClearsTheNotice() {
        let notice = HvfShareRootNotice()
        feed(notice, ["BVAGENT SHARE skip 휴가 영상.mov low-space 5000000000",
                      "BVAGENT SHARE skip big/disk.iso low-space 9000000000",
                      "BVAGENT SHARE skip notes.txt low-space 12"])
        // A whole file: the most recent file still waiting is named next.
        feed(notice, ["BVAGENT SHARE guest->host notes.txt bytes=12 t=40\r"])
        XCTAssertEqual(notice.lowSpace?.count, 2)
        XCTAssertEqual(notice.lowSpace?.last, "big/disk.iso")
        // A large file, by its done line alone.
        feed(notice, ["BVAGENT SHARE transfer-done guest->host big/disk.iso bytes=9000000000 ms=90000 mbps=100.0 t=50"])
        XCTAssertEqual(notice.lowSpace?.count, 1)
        XCTAssertEqual(notice.displayed?.text, "Mac 디스크 공간이 부족해 '휴가 영상.mov'을(를) 공유 폴더로 가져오지 않았습니다.")
        // A large file logs both lines; the name holds spaces.
        feed(notice, ["BVAGENT SHARE guest->host 휴가 영상.mov bytes=5000000000 t=60",
                      "BVAGENT SHARE transfer-done guest->host 휴가 영상.mov bytes=5000000000 ms=50000 mbps=100.0 t=60"])
        XCTAssertNil(notice.lowSpace)
        XCTAssertNil(notice.displayed)
    }

    @MainActor
    func testAStartADeleteOrAnotherFileLeavesItWaiting() {
        let notice = HvfShareRootNotice()
        // A name may hold " bytes=" too: it runs to the last one.
        feed(notice, ["BVAGENT SHARE skip 내 문서 bytes=1.txt low-space 300"])
        feed(notice, ["BVAGENT SHARE transfer guest->host 내 문서 bytes=1.txt bytes=300 t=1",
                      "BVAGENT SHARE del guest->host 내 문서 bytes=1.txt t=2",
                      "BVAGENT SHARE host->guest 내 문서 bytes=1.txt bytes=300 t=3",
                      "BVAGENT SHARE transfer-done host->guest 내 문서 bytes=1.txt bytes=300 ms=1 mbps=1.0 t=3",
                      "BVAGENT SHARE guest->host 내 bytes=300 t=4",
                      "BVAGENT SHARE guest->host 내 문서 bytes=1.txt bytes= t=5",
                      "BVAGENT SHARE guest->host 내 문서 bytes=1.txt bytes=30x t=5",
                      "BVAGENT SHARE guest->host 내 문서 bytes=1.txt bytes=\u{0663} t=5",
                      "BVAGENT SHARE transfer-done guest->host  bytes=300 ms=1 mbps=1.0 t=6",
                      "BVAGENT SHARE skip 내 문서 bytes=1.txt too-large 300"])
        XCTAssertEqual(notice.lowSpace?.count, 1)
        XCTAssertEqual(notice.lowSpace?.last, "내 문서 bytes=1.txt")
        feed(notice, ["BVAGENT SHARE guest->host 내 문서 bytes=1.txt bytes=300 t=7"])
        XCTAssertNil(notice.lowSpace)
    }
}
