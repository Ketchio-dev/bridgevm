import XCTest

@testable import BridgeVMApp

final class HvfGuestWindowProtocolTests: XCTestCase {
  private func b64(_ s: String) -> String { Data(s.utf8).base64EncodedString() }

  func testParsesAWindowLineIntoTheSharedModel() {
    let lines = ["WIN 66052 1234 100 50 800 600 \(b64("Untitled - Notepad"))", "WINEND"]

    let windows = HvfGuestWindowProtocol.parseWindowList(lines)

    XCTAssertEqual(windows.count, 1)
    XCTAssertEqual(windows[0].id, "66052")
    XCTAssertEqual(windows[0].title, "Untitled - Notepad")
    XCTAssertEqual(windows[0].pid, 1234)
    XCTAssertEqual(windows[0].source, "bvagent")
    XCTAssertEqual(windows[0].bounds, GuestToolsWindowBounds(x: 100, y: 50, width: 800, height: 600))
  }

  func testKoreanTitleSurvivesTheBase64RoundTrip() {
    let lines = ["WIN 1 1 0 0 10 10 \(b64("메모장 — 제목 없음"))", "WINEND"]

    XCTAssertEqual(HvfGuestWindowProtocol.parseWindowList(lines).first?.title, "메모장 — 제목 없음")
  }

  func testNegativeOriginIsKeptButNonPositiveSizeIsDropped() {
    // A window dragged partly off-screen has a negative origin and is real; a
    // zero-sized one is not usable as a proxy target.
    let lines = [
      "WIN 2 9 -5 -10 300 200 \(b64("offscreen"))",
      "WIN 3 9 0 0 0 200 \(b64("zero width"))",
      "WINEND",
    ]

    let windows = HvfGuestWindowProtocol.parseWindowList(lines)
    XCTAssertEqual(windows.map(\.id), ["2"])
    XCTAssertEqual(windows[0].bounds?.x, -5)
  }

  func testGarbageAndPostTerminatorLinesAreIgnored() {
    let lines = [
      "BVAGENT SERVICE noise",
      "WIN not-enough-fields",
      "WIN 4 1 0 0 10 10 not-base64!!",
      "WINEND",
      "WIN 5 1 0 0 10 10 \(b64("after end"))",
    ]

    XCTAssertEqual(HvfGuestWindowProtocol.parseWindowList(lines), [])
  }

  func testOutboundCommandsMatchTheAgentGrammar() {
    XCTAssertEqual(
      HvfGuestWindowProtocol.boundsCommand(
        id: "66052", bounds: GuestToolsWindowBounds(x: 10, y: 20, width: 640, height: 480)),
      "WINBOUNDS 66052 10 20 640 480")
    XCTAssertEqual(HvfGuestWindowProtocol.focusCommand(id: "66052"), "WINFOCUS 66052")
    XCTAssertEqual(HvfGuestWindowProtocol.closeCommand(id: "66052"), "WINCLOSE 66052")
  }
}
