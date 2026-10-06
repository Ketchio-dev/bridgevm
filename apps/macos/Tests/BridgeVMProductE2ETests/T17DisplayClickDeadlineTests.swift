import XCTest
@testable import BridgeVMProductE2E

final class T17DisplayClickDeadlineTests: XCTestCase {
    private let spot = CGPoint(x: 0.5, y: 0.5)
    private let target = T17DisplayClick.Target(value: "frame 1920x1080", focused: true,
        frame: CGRect(x: 100, y: 50, width: 1280, height: 800))

    func testReadyReadEndingAtOrAfterDeadlineCannotPost() {
        for elapsed in [1.0, 2.0] {
            var clock = Date(timeIntervalSince1970: 0), posts = 0
            XCTAssertThrowsError(try T17DisplayClick.click(at: spot, timeout: 1,
                read: { clock = Date(timeIntervalSince1970: elapsed); return self.target },
                post: { _ in posts += 1; return true }, now: { clock }, pause: {})) { error in
                    self.assertTimeout(error)
                }
            XCTAssertEqual(posts, 0)
        }
    }

    func testReadyReadEndingBeforeDeadlineStillPostsOnce() throws {
        var clock = Date(timeIntervalSince1970: 0), points: [CGPoint] = []
        try T17DisplayClick.click(at: spot, timeout: 1,
            read: { clock = Date(timeIntervalSince1970: 0.9); return self.target },
            post: { points.append($0); return true }, now: { clock }, pause: { XCTFail("ready read paused") })
        XCTAssertEqual(points, [CGPoint(x: 740, y: 450)])
    }

    func testPauseEndingAtDeadlineCannotReadOrPostAgain() {
        var clock = Date(timeIntervalSince1970: 0), reads = 0, posts = 0
        XCTAssertThrowsError(try T17DisplayClick.click(at: spot, timeout: 1,
            read: { reads += 1; return nil }, post: { _ in posts += 1; return true },
            now: { clock }, pause: { clock = Date(timeIntervalSince1970: 1) })) { self.assertTimeout($0) }
        XCTAssertEqual(reads, 1)
        XCTAssertEqual(posts, 0)
    }

    func testInvalidOrExhaustedBudgetDoesNotReadOrPost() {
        for timeout in [0.0, -1.0, .infinity, .nan] {
            XCTAssertThrowsError(try T17DisplayClick.click(at: spot, timeout: timeout,
                read: { XCTFail("invalid budget read"); return self.target },
                post: { _ in XCTFail("invalid budget posted"); return true },
                now: { Date(timeIntervalSince1970: 0) }, pause: { XCTFail("invalid budget paused") })) {
                    self.assertTimeout($0)
                }
        }
    }

    private func assertTimeout(_ error: Error) {
        XCTAssertEqual(error as? T17Blocker, .init(code: "ui-element-missing",
            detail: "guest display surface did not present a frame in the focused window"))
    }
}
