import XCTest
@testable import BridgeVMProductE2E

final class T17DisplayClickTests: XCTestCase {
    private let frame = CGRect(x: 100, y: 50, width: 1280, height: 800)

    func testClickNeedsAPresentedFrameInTheFocusedWindow() {
        XCTAssertNil(T17DisplayClick.point(for: .init(value: "no-frame", focused: true, frame: frame)))
        XCTAssertNil(T17DisplayClick.point(for: .init(value: nil, focused: true, frame: frame)))
        XCTAssertNil(T17DisplayClick.point(for: .init(value: "frame 1920x1080", focused: false, frame: frame)))
        XCTAssertNil(T17DisplayClick.point(for: .init(value: "frame 1920x1080", focused: true, frame: nil)))
        XCTAssertNil(T17DisplayClick.point(for: .init(value: "frame 1920x1080", focused: true, frame: .zero)))
        XCTAssertEqual(T17DisplayClick.point(for: .init(value: "frame 1920x1080", focused: true, frame: frame)),
                       CGPoint(x: 740, y: 450))
    }

    func testClickWaitsForTheFirstFrameAndPostsOnce() throws {
        var reads = 0, posted: [CGPoint] = []
        try T17DisplayClick.click(timeout: 5, read: {
            reads += 1
            return .init(value: reads < 3 ? "no-frame" : "frame 1920x1080", focused: true, frame: frame)
        }, post: { posted.append($0); return true })
        XCTAssertEqual(reads, 3)
        XCTAssertEqual(posted, [CGPoint(x: 740, y: 450)])
    }

    func testClickWithoutAFrameFailsWithoutPosting() {
        var posted = 0
        XCTAssertThrowsError(try T17DisplayClick.click(timeout: 0.3, read: {
            .init(value: "no-frame", focused: true, frame: frame)
        }, post: { _ in posted += 1; return true })) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest display surface did not present a frame in the focused window")
        }
        XCTAssertEqual(posted, 0)
    }

    func testUnpostableClickFails() {
        XCTAssertThrowsError(try T17DisplayClick.click(timeout: 1, read: {
            .init(value: "frame 1920x1080", focused: true, frame: frame)
        }, post: { _ in false })) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "display click events could not be created")
        }
    }
}
