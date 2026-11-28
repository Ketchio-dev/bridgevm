import XCTest
@testable import BridgeVMControl

/// A Mac file Windows refused (`put-error`) or names Windows can't keep apart (`skip ... alias`) were
/// only run.log lines: the file never appeared in Windows and nothing said why. The runtime card now
/// says so.
@MainActor
final class HvfShareMissingFileNoticeTests: XCTestCase {
    private func feed(_ notice: HvfShareRootNotice, _ lines: [String]) {
        for event in BvAgentEvent.parse(lines: lines) { notice.consume(event) }
    }

    func testAFileWindowsRefusedIsNamedUntilItReachesWindows() {
        let notice = HvfShareRootNotice()
        // Windows' message is in its own language and quotes the path; the name holds " ERR " too.
        feed(notice, ["BVAGENT SHARE put-error Q3 ERR 보고서.docx ERR PUT Der Prozess kann nicht auf die Datei "
                      + "\"C:\\bridgevm-share\\Q3 ERR 보고서.docx\" zugreifen, da sie von einem anderen Prozess verwendet wird.\r"])
        XCTAssertEqual(notice.displayed?.text, "“Q3 ERR 보고서.docx” couldn’t be copied from your Mac to Windows.")
        XCTAssertEqual(notice.displayed?.help,
                       "BridgeVM tries again. Close the file if a Windows app has it open, or free up space in Windows.")
        feed(notice, ["BVAGENT SHARE put-error big/disk.iso ERR LPUTBEG disk-full",
                      "BVAGENT SHARE put-error a b.bin PUTOK QzpcYS5iaW4= 5",
                      "BVAGENT SHARE put-error ERR PUT empty name", "BVAGENT SHARE put-error x.txt ERR lower case"])
        XCTAssertEqual(notice.displayed?.text, "Some files from your Mac couldn’t be copied to Windows, such as “a b.bin”.")
        // A file reaching the Mac, or another file reaching Windows, leaves it waiting.
        feed(notice, ["BVAGENT SHARE guest->host a b.bin bytes=5 t=1", "BVAGENT SHARE host->guest c.txt bytes=1 t=2",
                      "BVAGENT SHARE host->guest a b.bin bytes=5 t=3"])
        XCTAssertEqual(notice.displayed?.text, "Some files from your Mac couldn’t be copied to Windows, such as “big/disk.iso”.")
        // A Windows-side problem outranks it; a stop outranks everything.
        feed(notice, ["BVAGENT SHARE write-refused d.txt Input/output error (os error 5)"])
        XCTAssertEqual(notice.displayed?.text, "Files from Windows aren’t reaching the shared folder on your Mac.")
        feed(notice, ["BVAGENT SHARE guest->host d.txt bytes=1 t=4", "BVAGENT SHARE host->guest big/disk.iso bytes=9 t=5"])
        XCTAssertEqual(notice.displayed?.text, "“Q3 ERR 보고서.docx” couldn’t be copied from your Mac to Windows.")
        feed(notice, ["BVAGENT SHARE disabled failed-destination-limit"])
        XCTAssertEqual(notice.displayed?.text, "Folder sync stopped for this session after repeated write failures on your Mac.")
        notice.consume(.unknown(HvfGuestResetRecord.processRecreation))
        XCTAssertNil(notice.displayed)
        feed(notice, ["BVAGENT SHARE put-error Q3 ERR 보고서.docx ERR PUT Zugriff verweigert",
                      "BVAGENT SHARE host->guest Q3 ERR 보고서.docx bytes=7 t=6"])
        XCTAssertNil(notice.displayed)
    }

    func testNamesWindowsCanNotKeepApartAreNamedUntilOneSyncs() {
        let notice = HvfShareRootNotice()
        feed(notice, [#"BVAGENT SHARE skip docs/A.txt alias ["docs/A.txt", "docs/a.txt"]"#])
        XCTAssertEqual(notice.displayed?.text,
                       "Windows can’t keep both “docs/A.txt” and “docs/a.txt”, because Windows ignores letter case in names.")
        XCTAssertEqual(notice.displayed?.help, "Rename one of them on your Mac.")
        // A name may hold quotes and the marker itself; the list is Rust's `{:?}`.
        feed(notice, [#"BVAGENT SHARE skip my "notes" alias [1] alias ["my \"notes\" alias [1]", "my \"notes\" alias [1]."]"# + "\r"])
        XCTAssertEqual(notice.displayed?.text,
                       "Windows can’t keep both “my \"notes\" alias [1]” and “my \"notes\" alias [1].”, because Windows sees them as the same name.")
        feed(notice, [#"BVAGENT SHARE skip old./a.txt alias ["old./a.txt"]"#])
        XCTAssertEqual(notice.displayed?.text, "Windows can’t use “old./a.txt”, because a folder in its path ends in a dot or space.")
        XCTAssertEqual(notice.displayed?.help, "Rename the folder on your Mac.")
        // A file Windows refused outranks it.
        feed(notice, ["BVAGENT SHARE put-error b.txt ERR PUT denied"])
        XCTAssertEqual(notice.displayed?.text, "“b.txt” couldn’t be copied from your Mac to Windows.")
        feed(notice, ["BVAGENT SHARE host->guest b.txt bytes=1 t=1"])
        XCTAssertEqual(notice.displayed?.text, "Windows can’t use “old./a.txt”, because a folder in its path ends in a dot or space.")
        // Lines that are not the runtime's list leave it as it is.
        feed(notice, [#"BVAGENT SHARE skip x alias ["y"]"#, #"BVAGENT SHARE skip x alias ["x""#,
                      #"BVAGENT SHARE skip x alias ["x"] "#, #"BVAGENT SHARE skip  alias [""]"#])
        XCTAssertEqual(notice.displayed?.text, "Windows can’t use “old./a.txt”, because a folder in its path ends in a dot or space.")
        feed(notice, ["BVAGENT SHARE guest->host old./a.txt bytes=1 t=2"])
        XCTAssertNil(notice.displayed)
    }
}
