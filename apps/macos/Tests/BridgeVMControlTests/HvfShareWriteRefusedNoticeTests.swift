import XCTest
@testable import BridgeVMControl

/// A file the Mac refused to save from Windows was only a log line (`write-refused`): the file stayed in
/// Windows and never appeared on the Mac. The runtime card now says so until a file arrives again.
@MainActor
final class HvfShareWriteRefusedNoticeTests: XCTestCase {
    func testARefusedWriteSaysWhyUntilAFileFromWindowsArrives() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown("BVAGENT SHARE write-refused 보고서 최종.docx execute bits refused: share filesystem keeps them on guest files\r"))
        XCTAssertEqual(notice.displayed?.text, "Files from Windows aren’t reaching the shared folder on your Mac.")
        XCTAssertEqual(notice.displayed?.help.hasSuffix("share a folder on your Mac’s own disk instead."), true)
        notice.consume(.unknown("BVAGENT SHARE write-refused a b.txt Permission denied (os error 13)"))
        XCTAssertEqual(notice.writeRefused, .noPermission)
        XCTAssertEqual(notice.displayed?.help, "Check in Finder that you can write to the folder. Sync will retry automatically.")
        notice.consume(.unknown("BVAGENT SHARE write-refused x.bin quarantine refused: msdos/exfat mark did not read back"))
        XCTAssertEqual(notice.writeRefused, .unsafeDrive)
        notice.consume(.shareEvent(kind: .guestToHost, path: "c.txt", bytes: 3, tMs: 1))
        XCTAssertNil(notice.displayed, "a file from Windows arrived")
        notice.consume(.unknown("BVAGENT SHARE write-refused d.txt Input/output error (os error 5)"))
        XCTAssertEqual(notice.writeRefused, .other)
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.writeRefused)
    }

    func testOutOfSpaceKeepsItsOwnNotice() {
        let notice = HvfShareRootNotice()
        notice.consume(.unknown("BVAGENT SHARE write-refused big.iso No space left on device (os error 28)"))
        XCTAssertNil(notice.writeRefused)
        XCTAssertNil(HvfShareWriteRefused(line: "BVAGENT SHARE skip a.bin low-space 5"))
    }
}
