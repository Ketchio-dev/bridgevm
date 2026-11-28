import Combine
import XCTest
@testable import BridgeVMControl

/// Shared-folder failures that were only run.log lines, or said something untrue: each shows one
/// sentence, once, from the runtime's exact line (agent_console share_listing_pause.rs,
/// host_destination.rs, share_root.rs).
final class HvfShareSyncFailureNoticeTests: XCTestCase {
    @MainActor
    func testAWindowsFolderWindowsCanNotOpenIsNamedOnce() {
        let notice = HvfShareRootNotice()
        var stream = BvAgentEvent.Stream()
        var changes = 0
        let sink = notice.objectWillChange.sink { changes += 1 }
        defer { sink.cancel() }
        // Before a first listing: an ls-error at every poll, the folder made, then one pause line.
        let denied = "BVAGENT SHARE ls-error ERR LSR Der Zugriff auf den Pfad \"C:\\bridgevm-share\\\" wurde verweigert."
        for event in stream.parse([denied, "BVAGENT SHARE create-guest-folder", denied,
                                   "BVAGENT SHARE paused windows-folder-unreadable C:\\bridgevm-share\r", denied, denied]) {
            notice.consume(event)
        }
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because Windows can’t open its shared folder (C:\\bridgevm-share).")
        XCTAssertEqual(notice.displayed?.help, "If you renamed, moved or deleted the folder in Windows, put it back. Sync resumes on its own.")
        XCTAssertEqual(changes, 1)
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        XCTAssertNil(notice.displayed)

        // A folder chosen in Settings is read to the end of the line; one that can't be shown isn't.
        notice.consume(.unknown("BVAGENT SHARE paused windows-folder-unreadable D:\\Shared files"))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because Windows can’t open its shared folder (D:\\Shared files).")
        notice.consume(.unknown("BVAGENT SHARE resumed"))
        notice.consume(.unknown("BVAGENT SHARE paused windows-folder-unreadable D:\\\u{202E}eraf"))
        XCTAssertEqual(notice.displayed?.text, "Folder sync paused because Windows can’t open its shared folder.")
    }

    @MainActor
    func testAWindowsVersionTheMacCouldNotKeepIsNamedOnceUntilAGuestReset() {
        let notice = HvfShareRootNotice()
        var stream = BvAgentEvent.Stream()
        var changes = 0
        let sink = notice.objectWillChange.sink { changes += 1 }
        defer { sink.cancel() }
        // The name holds spaces and even the marker: it runs to the end of the line.
        let name = "Q3 kept-mac= 보고서.docx"
        let lost = "BVAGENT SHARE conflict windows-copy-not-kept No space left on device (os error 28) kept-mac=\(name)"
        for event in stream.parse([lost + "\r", "BVAGENT SHARE skip docs/\(name) mac-changed", lost]) {
            notice.consume(event)
        }
        let text = "BridgeVM couldn’t keep the Windows version of “\(name)” next to the Mac version, so the Mac version replaces it in Windows."
        XCTAssertEqual(notice.displayed?.text, text)
        XCTAssertEqual(notice.displayed?.help, "Check your Mac’s free space so BridgeVM can keep both versions next time.")
        XCTAssertEqual(changes, 1)
        XCTAssertNil(notice.lastRecoveredCopy)

        // A file waiting for space shows first, until it arrives.
        notice.consume(.unknown("BVAGENT SHARE skip big.iso low-space 4096"))
        XCTAssertEqual(notice.displayed?.text, "Your Mac needs more space to receive “big.iso” from Windows.")
        notice.consume(.unknown("BVAGENT SHARE transfer-done guest->host big.iso bytes=4096 ms=1 mbps=4.1 t=9"))
        XCTAssertEqual(notice.displayed?.text, text)

        notice.consume(.unknown("BVAGENT SHARE conflict windows-copy-not-kept Input/output error (os error 5) kept-mac=\u{202E}xcod.exe"))
        XCTAssertEqual(notice.displayed?.text, "BridgeVM couldn’t keep the Windows version of a file next to the Mac version, so the Mac version replaces it in Windows.")
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.displayed)

        // Guest text and command output are not the runtime's lines.
        for event in stream.parse(["| " + lost, "BVAGENT CMD type log.txt exit=0", lost, "BVAGENT END type log.txt"]) {
            notice.consume(event)
        }
        XCTAssertNil(notice.displayed)
    }

    @MainActor
    func testABlockedFolderSaysItIsNotWhereItWasOnTheMacOnce() {
        let notice = HvfShareRootNotice()
        var changes = 0
        let sink = notice.objectWillChange.sink { changes += 1 }
        defer { sink.cancel() }
        notice.consume(.unknown("BVAGENT SHARE root-state blocked"))
        notice.consume(.unknown("BVAGENT SHARE root-state blocked\r"))
        XCTAssertEqual(notice.displayed?.text, "Some changes are waiting because BridgeVM can’t find the shared folder on your Mac.")
        XCTAssertEqual(notice.displayed?.help,
                       "If you renamed or moved it, put it back and sync resumes on its own. Otherwise, shut down the VM and choose another folder.")
        XCTAssertEqual(changes, 1)
        notice.consume(.unknown("BVAGENT SHARE root-state ready"))
        XCTAssertNil(notice.displayed)
    }
}
