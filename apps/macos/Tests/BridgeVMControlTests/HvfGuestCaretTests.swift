#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// The guest's caret answer (guest bytes) and its mapping onto the display view.
final class HvfGuestCaretTests: XCTestCase {
    func testOnlyFourPlainNumbersWithinADesktopAreACaret() {
        XCTAssertEqual(HvfGuestCaret.parse("BVCARET 120 340 2 18"), CGRect(x: 120, y: 340, width: 2, height: 18))
        XCTAssertEqual(HvfGuestCaret.parse("BVCARET 0 0 0 1"), CGRect(x: 0, y: 0, width: 0, height: 1))
        XCTAssertEqual(HvfGuestCaret.parse("BVCARET 16384 16384 16384 16384"),
                       CGRect(x: 16384, y: 16384, width: 16384, height: 16384))
        for line in ["BVCARET none", "BVCARET", "BVCARET -1 2 3 4", "BVCARET 1 -2 3 4", "BVCARET +1 2 3 4",
                     "BVCARET 16385 0 1 1", "BVCARET 0 99999 1 1", "BVCARET 0 0 1 0", "BVCARET 1 2 3",
                     "BVCARET 1 2 3 4 5", "BVCARET 1  2 3 4", "BVCARET 1 2 3 4 ", " BVCARET 1 2 3 4",
                     "BVCARET 1.5 2 3 4", "BVCARET 0x1 2 3 4", "BVCARET 1e3 2 3 4", "BVCARET ١ 2 3 4",
                     "BVCARET 000001 2 3 4", "bvcaret 1 2 3 4", "BVCARET\t1 2 3 4", "BVCARET 99999999999999999999 1 1 1"] {
            XCTAssertNil(HvfGuestCaret.parse(line), line)
        }
    }

    func testTheAnswerIsTheFrameForThisQuestionsID() {
        let start = Date(timeIntervalSince1970: 1000)
        let id = UUID()
        var request = HvfGuestCaretRequest(now: start, id: id)
        XCTAssertEqual(request.command, "CARET " + id.uuidString)
        let other = "CARET " + UUID().uuidString
        // An earlier question's reply, and lines outside a frame, are not this answer.
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(other) exit=0", "BVCARET 1 2 3 4", "BVAGENT END \(other)",
                                            "BVCARET 5 6 7 8"], now: start))
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0\r"], now: start))
        XCTAssertEqual(request.consume(lines: ["BVCARET 10 20 2 18\r", "BVAGENT END \(request.command)"], now: start),
                       .caret(CGRect(x: 10, y: 20, width: 2, height: 18)))
    }

    func testNoneAnErrorAMalformedLineARestartOrNoAnswerIsNoCaret() {
        let start = Date(timeIntervalSince1970: 1000)
        func answer(_ body: [String], now: Date = Date(timeIntervalSince1970: 1000)) -> HvfGuestCaretRequest.Answer? {
            var request = HvfGuestCaretRequest(now: start)
            return request.consume(lines: body.map { $0.replacingOccurrences(of: "$C", with: request.command) }, now: now)
        }
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=0", "BVCARET none", "BVAGENT END $C"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=1", "BVCARET unavailable", "BVAGENT END $C"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=255", "unknown token: CARET", "BVAGENT END $C"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=0", "BVCARET -5 0 1 1"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=0", "BVAGENT END $C"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=0", "BVAGENT READY host=x t=1", "BVCARET 1 2 3 4"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertEqual(answer(["BVAGENT SERVICE start t=9"]), HvfGuestCaretRequest.Answer.none)
        XCTAssertNil(answer([], now: start.addingTimeInterval(0.29)))
        XCTAssertEqual(answer([], now: start.addingTimeInterval(HvfGuestCaretRequest.timeout)), HvfGuestCaretRequest.Answer.none)
        // Lines read late still count: the deadline is checked after them.
        XCTAssertEqual(answer(["BVAGENT CMD $C exit=0", "BVCARET 1 2 3 4"], now: start.addingTimeInterval(1)),
                       .caret(CGRect(x: 1, y: 2, width: 3, height: 4)))
    }

    func testTheCaretMapsAsTheFrameIsShownLetterboxedAndScaled() {
        // 1920x1080 in a 960x600 view: half size, 30 pt bars above and below.
        XCTAssertEqual(HvfGuestCaret.viewRect(CGRect(x: 100, y: 200, width: 2, height: 40),
                                              viewSize: CGSize(width: 960, height: 600), imageSize: CGSize(width: 1920, height: 1080)),
                       CGRect(x: 50, y: 130, width: 1, height: 20))
        // A tall image: bars left and right.
        XCTAssertEqual(HvfGuestCaret.viewRect(CGRect(x: 0, y: 0, width: 4, height: 10),
                                              viewSize: CGSize(width: 400, height: 200), imageSize: CGSize(width: 100, height: 100)),
                       CGRect(x: 100, y: 0, width: 8, height: 20))
        // Retina 1:1, a frame of the view's backing size: one guest pixel is half a point.
        XCTAssertEqual(HvfGuestCaret.viewRect(CGRect(x: 600, y: 400, width: 2, height: 36),
                                              viewSize: CGSize(width: 1280, height: 800), imageSize: CGSize(width: 2560, height: 1600)),
                       CGRect(x: 300, y: 200, width: 1, height: 18))
        // Clipped to the image; off it, or with no image, there is no caret.
        XCTAssertEqual(HvfGuestCaret.viewRect(CGRect(x: 90, y: 95, width: 20, height: 20),
                                              viewSize: CGSize(width: 100, height: 100), imageSize: CGSize(width: 100, height: 100)),
                       CGRect(x: 90, y: 95, width: 10, height: 5))
        XCTAssertNil(HvfGuestCaret.viewRect(CGRect(x: 1920, y: 0, width: 1, height: 10),
                                            viewSize: CGSize(width: 960, height: 600), imageSize: CGSize(width: 1920, height: 1080)))
        XCTAssertNil(HvfGuestCaret.viewRect(CGRect(x: 0, y: 1080, width: 1, height: 10),
                                            viewSize: CGSize(width: 960, height: 600), imageSize: CGSize(width: 1920, height: 1080)))
        XCTAssertNil(HvfGuestCaret.viewRect(CGRect(x: 0, y: 0, width: 1, height: 10), viewSize: CGSize(width: 960, height: 600), imageSize: .zero))
    }

    func testTheOverlayGoesJustBelowTheCaretAndStaysInsideTheView() {
        let view = CGSize(width: 400, height: 300), overlay = CGSize(width: 60, height: 22)
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 100, y: 50, width: 1, height: 18), overlay: overlay, view: view),
                       CGPoint(x: 100, y: 70))
        // Near the right edge it moves left; at the left edge it keeps the margin.
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 380, y: 50, width: 1, height: 18), overlay: overlay, view: view).x, 328)
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 2, y: 50, width: 1, height: 18), overlay: overlay, view: view).x, 12)
        // No room below: above the caret. At the very top: inside the margin.
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 100, y: 270, width: 1, height: 18), overlay: overlay, view: view).y, 246)
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 100, y: 0, width: 1, height: 2), overlay: overlay, view: view).y, 12)
        // An overlay wider than the view keeps the left margin.
        XCTAssertEqual(HvfGuestCaret.overlayOrigin(caret: CGRect(x: 200, y: 50, width: 1, height: 18),
                                                   overlay: CGSize(width: 500, height: 22), view: view).x, 12)
    }
}
#endif
